#!/usr/bin/env bash
set -euo pipefail

# --- Defaults ---
SAMPLES=10   # number of measured runs (after warmup)
WARMUP=1     # warmup runs to discard

usage() {
  cat <<EOF
Usage:
  $0 [options] <TARGET_URL> <NOTARY_URL>

Positional:
  TARGET_URL     Fully qualified URL (e.g., https://api.example.com/health)
  NOTARY_URL     Fully qualified URL (e.g., https://notary.pse.dev/)

Options:
  -n, --samples N       Number of samples (default: ${SAMPLES})
  -w, --warmup N        Warm-up runs to discard (default: ${WARMUP})
  -h, --help            Show help

Example:
  $0 -n 20 https://example.com/ https://notary.pse.dev/
EOF
}

# --- Parse args ---
if [ "$#" -lt 2 ]; then usage; exit 1; fi

OPTS=()
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    -n|--samples) SAMPLES="$2"; shift 2 ;;
    -w|--warmup)  WARMUP="$2"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    http*://*)    OPTS+=("$1"); shift ;;
    *) echo "Unknown arg: $1"; usage; exit 1 ;;
  esac
done

if [ "${#OPTS[@]}" -lt 2 ]; then usage; exit 1; fi
TARGET_URL="${OPTS[0]}"
NOTARY_URL="${OPTS[1]}"

# --- Helpers ---
fmt_ms() { awk 'BEGIN{printf "%.2f", ARGV[1]*1000}' "$1"; }

measure_latency_h1() {
  local url="$1" label="$2"
  echo "🧪  ${label} | HTTP/1.1 | URL=${url}"
  echo "    Samples=${SAMPLES} (warmup ${WARMUP})"
  echo

  local runs=$((SAMPLES+WARMUP))
  local sum_app=0 sumsq_app=0
  local min_app=0 max_app=0
  local first_sample=1

  for i in $(seq 1 "${runs}"); do
    read t_dns t_tcp t_tls t_pre t_ttfb t_total http_ver remote_ip <<<"$(
      curl --silent --output /dev/null \
           --http1.1 --no-keepalive --head "$url" \
           -w '%{time_namelookup} %{time_connect} %{time_appconnect} %{time_pretransfer} %{time_starttransfer} %{time_total} %{http_version} %{remote_ip}\n'
    )" || { echo "    curl failed for ${label}; skipping run"; continue; }

    app_rtt=$(awk "BEGIN{print ${t_ttfb} - ${t_pre}}")     # after TLS complete
    tls_time=$(awk "BEGIN{print ${t_tls} - ${t_tcp}}")

    if [ "$i" -gt "$WARMUP" ]; then
      sum_app=$(awk "BEGIN{print ${sum_app} + ${app_rtt}}")
      sumsq_app=$(awk "BEGIN{print ${sumsq_app} + (${app_rtt}*${app_rtt})}")

      if [ "$first_sample" -eq 1 ]; then
        min_app="$app_rtt"
        max_app="$app_rtt"
        first_sample=0
      else
        awk -v a="${app_rtt}" -v m="${min_app}" 'BEGIN{if(a<m) print a; else print m}' >/tmp/min.$$
        min_app="$(cat /tmp/min.$$)"; rm -f /tmp/min.$$
        awk -v a="${app_rtt}" -v m="${max_app}" 'BEGIN{if(a>m) print a; else print m}' >/tmp/max.$$
        max_app="$(cat /tmp/max.$$)"; rm -f /tmp/max.$$
      fi
    fi

    tag=$([ "$i" -le "$WARMUP" ] && echo "(warmup)" || echo "        ")
    printf ' • run %2d %-9s → App-RTT %7.2f ms  [DNS %.2f | TCP %.2f | TLS %.2f | App %.2f | tot %.2f]  ip=%s h%s\n' \
      "$i" "$tag" "$(fmt_ms "$app_rtt")" \
      "$(fmt_ms "$t_dns")" "$(fmt_ms "$t_tcp")" "$(fmt_ms "$tls_time")" \
      "$(fmt_ms "$app_rtt")" "$(fmt_ms "$t_total")" "$remote_ip" "$http_ver"
  done

  local n="$SAMPLES"
  local avg=$(awk "BEGIN{print ${sum_app}/${n}}")
  local var=$(awk "BEGIN{print (${sumsq_app}/${n}) - (${avg}*${avg})}")
  local sd=$(awk "BEGIN{if(${var}<0){print 0}else{print sqrt(${var})}}")

  echo
  echo "📊  ${label} (HTTP/1.1) — ${n} runs"
  echo "    ▸ App-RTT avg : $(fmt_ms "$avg") ms"
  echo "    ▸ App-RTT min : $(fmt_ms "$min_app") ms"
  echo "    ▸ App-RTT max : $(fmt_ms "$max_app") ms"
  echo "    ▸ App-RTT σ   : $(fmt_ms "$sd") ms"
  echo
}

echo "───────────────────────────────────────────────────────────────"
measure_latency_h1 "$TARGET_URL" "Target Server"
echo "───────────────────────────────────────────────────────────────"
measure_latency_h1 "$NOTARY_URL" "Notary Server"
echo "───────────────────────────────────────────────────────────────"
echo "Done."
