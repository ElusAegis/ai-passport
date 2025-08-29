#!/usr/bin/env bash
set -euo pipefail

# --- Defaults (override with flags) ---
SAMPLES=1
WARMUP=0
SERVER_ID=""

usage() {
  cat <<EOF
Usage:
  $0 [options]

Options:
  -n, --samples N        Number of measured samples (default: ${SAMPLES})
  -w, --warmup N         Warm-up runs to discard (default: ${WARMUP})
  -s, --server-id ID     Use specific server ID (python speedtest-cli: --server ID; Ookla: -s ID)
  -h, --help             Show help

Notes:
  • Supports BOTH schemas:
      - Python speedtest-cli:    { "ping": <ms>, "download": <bits/s>, "upload": <bits/s>, ... }
      - Ookla native CLI JSON:   { "ping": { "latency": <ms> }, "download": { "bandwidth": <bytes/s> }, ... }
  • Suppresses Python DeprecationWarning from speedtest-cli via PYTHONWARNINGS.
EOF
}

# --- Parse args ---
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    -n|--samples) SAMPLES="$2"; shift 2 ;;
    -w|--warmup)  WARMUP="$2"; shift 2 ;;
    -s|--server-id) SERVER_ID="$2"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    *) echo "Unknown arg: $1"; usage; exit 1 ;;
  esac
done

# --- Deps ---
if ! command -v speedtest >/dev/null 2>&1; then
  echo "❌ 'speedtest' not found. Install either 'speedtest-cli' (python) or Ookla CLI."
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "❌ 'jq' is required."
  exit 1
fi

# --- Helpers ---
to_mbps() { awk 'BEGIN{printf "%.2f", ARGV[1]/1000000}' "$1"; }          # bits/s  -> Mbps
bytesps_to_mbps() { awk 'BEGIN{printf "%.2f", (ARGV[1]*8)/1000000}' "$1"; } # bytes/s -> Mbps
fmt_ms() { awk 'BEGIN{printf "%.2f", ARGV[1]}' "$1"; }

sum_dl=0; sumsq_dl=0; min_dl=0; max_dl=0
sum_ul=0; sumsq_ul=0; min_ul=0; max_ul=0
sum_ping=0; sumsq_ping=0; min_ping=0; max_ping=0
first_sample=1
runs=$((SAMPLES+WARMUP))

echo "🌐  Measuring bandwidth using Speedtest…"
echo "    Samples=${SAMPLES} (warmup ${WARMUP})  ServerID=${SERVER_ID:-auto}"
echo

for i in $(seq 1 "${runs}"); do
  # Suppress python DeprecationWarning from speedtest-cli
  export PYTHONWARNINGS="ignore::DeprecationWarning"

  # Try both CLIs:
  # 1) Ookla native: supports --format=json (new) or --format=json-pretty; server pinned with -s
  # 2) Python speedtest-cli: supports --json; server pinned with --server
  json=""
  if speedtest --help 2>&1 | grep -q -- '--format='; then
    # Ookla CLI
    if [ -n "$SERVER_ID" ]; then
      json="$(speedtest --json 2>/dev/null || true)"
    else
      json="$(speedtest --accept-license --accept-gdpr --format=json 2>/dev/null || true)"
    fi
  else
    # python speedtest-cli
    if [ -n "$SERVER_ID" ]; then
      json="$(speedtest --server "$SERVER_ID" --json 2>/dev/null || true)"
    else
      json="$(speedtest --json 2>/dev/null || true)"
    fi
  fi

  if [ -z "$json" ]; then
    echo "    speedtest run failed; skipping…"
    continue
  fi

  # Extract values robustly for both schemas:
  # ping_ms: number (python) OR object.latency (Ookla)
  ping_ms="$(printf '%s' "$json" | jq -r 'if (.ping|type)=="object" then .ping.latency else .ping end')"
  # download: bits/s (python) OR object.bandwidth bytes/s (Ookla)
  dl_is_bytes="$(printf '%s' "$json" | jq -r 'if (.download|type)=="object" then .download.bandwidth else empty end')"
  ul_is_bytes="$(printf '%s' "$json" | jq -r 'if (.upload|type)=="object" then .upload.bandwidth else empty end')"

  if [ -n "$dl_is_bytes" ]; then
    dl_mbps="$(bytesps_to_mbps "$dl_is_bytes")"
    ul_mbps="$(bytesps_to_mbps "$ul_is_bytes")"
  else
    dl_bits="$(printf '%s' "$json" | jq -r '.download')"
    ul_bits="$(printf '%s' "$json" | jq -r '.upload')"
    dl_mbps="$(to_mbps "$dl_bits")"
    ul_mbps="$(to_mbps "$ul_bits")"
  fi

  tag=$([ "$i" -le "$WARMUP" ] && echo "(warmup)" || echo "        ")
  printf ' • run %2d %-9s → Down %7.2f Mbps | Up %7.2f Mbps | Ping %6.2f ms\n' \
    "$i" "$tag" "$dl_mbps" "$ul_mbps" "$(fmt_ms "$ping_ms")"

  # Skip accumulation during warmup
  if [ "$i" -le "$WARMUP" ]; then continue; fi

  # Convert to raw units for stats: use bits/s for dl/ul, ms for ping
  if [ -n "$dl_is_bytes" ]; then
    dl_bits_raw=$(awk "BEGIN{print ${dl_is_bytes}*8}")
    ul_bits_raw=$(awk "BEGIN{print ${ul_is_bytes}*8}")
  else
    dl_bits_raw="$dl_bits"
    ul_bits_raw="$ul_bits"
  fi

  sum_dl=$(awk "BEGIN{print ${sum_dl} + ${dl_bits_raw}}")
  sumsq_dl=$(awk "BEGIN{print ${sumsq_dl} + (${dl_bits_raw}*${dl_bits_raw})}")
  sum_ul=$(awk "BEGIN{print ${sum_ul} + ${ul_bits_raw}}")
  sumsq_ul=$(awk "BEGIN{print ${sumsq_ul} + (${ul_bits_raw}*${ul_bits_raw})}")
  sum_ping=$(awk "BEGIN{print ${sum_ping} + ${ping_ms}}")
  sumsq_ping=$(awk "BEGIN{print ${sumsq_ping} + (${ping_ms}*${ping_ms})}")

  if [ "$first_sample" -eq 1 ]; then
    min_dl="$dl_bits_raw"; max_dl="$dl_bits_raw"
    min_ul="$ul_bits_raw"; max_ul="$ul_bits_raw"
    min_ping="$ping_ms";  max_ping="$ping_ms"
    first_sample=0
  else
    min_dl="$(awk -v a="${dl_bits_raw}" -v m="${min_dl}" 'BEGIN{if(a<m) print a; else print m}')"
    max_dl="$(awk -v a="${dl_bits_raw}" -v m="${max_dl}" 'BEGIN{if(a>m) print a; else print m}')"
    min_ul="$(awk -v a="${ul_bits_raw}" -v m="${min_ul}" 'BEGIN{if(a<m) print a; else print m}')"
    max_ul="$(awk -v a="${ul_bits_raw}" -v m="${max_ul}" 'BEGIN{if(a>m) print a; else print m}')"
    min_ping="$(awk -v a="${ping_ms}" -v m="${min_ping}" 'BEGIN{if(a<m) print a; else print m}')"
    max_ping="$(awk -v a="${ping_ms}" -v m="${max_ping}" 'BEGIN{if(a>m) print a; else print m}')"
  fi
done

if [ "$first_sample" -eq 1 ]; then
  echo
  echo "⚠️  No successful measured samples collected (after warmup)."
  exit 1
fi

n="$SAMPLES"

avg_dl=$(awk "BEGIN{print ${sum_dl}/${n}}")
var_dl=$(awk "BEGIN{print (${sumsq_dl}/${n}) - (${avg_dl}*${avg_dl})}")
sd_dl=$(awk "BEGIN{if(${var_dl}<0){print 0}else{print sqrt(${var_dl})}}")

avg_ul=$(awk "BEGIN{print ${sum_ul}/${n}}")
var_ul=$(awk "BEGIN{print (${sumsq_ul}/${n}) - (${avg_ul}*${avg_ul})}")
sd_ul=$(awk "BEGIN{if(${var_ul}<0){print 0}else{print sqrt(${var_ul})}}")

avg_ping=$(awk "BEGIN{print ${sum_ping}/${n}}")
var_ping=$(awk "BEGIN{print (${sumsq_ping}/${n}) - (${avg_ping}*${avg_ping})}")
sd_ping=$(awk "BEGIN{if(${var_ping}<0){print 0}else{print sqrt(${var_ping})}}")

echo
echo "📊  Internet bandwidth summary — ${n} runs"
echo "    ▸ Download avg : $(to_mbps "$avg_dl") Mbps   (min $(to_mbps "$min_dl"), max $(to_mbps "$max_dl"), σ $(to_mbps "$sd_dl"))"
echo "    ▸ Upload   avg : $(to_mbps "$avg_ul") Mbps   (min $(to_mbps "$min_ul"), max $(to_mbps "$max_ul"), σ $(to_mbps "$sd_ul"))"
echo "    ▸ Ping     avg : $(fmt_ms "$avg_ping") ms     (min $(fmt_ms "$min_ping"), max $(fmt_ms "$max_ping"), σ $(fmt_ms "$sd_ping"))"
echo
