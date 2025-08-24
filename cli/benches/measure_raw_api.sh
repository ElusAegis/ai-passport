#!/bin/bash
set -euo pipefail
export LC_ALL=C

# --- Config ---
ROUNDS=10
REDPILL_URL="https://api.red-pill.ai/v1/chat/completions"
POA_URL="https://api.proof-of-autonomy.elusaegis.xyz:3000/v1/chat/completions"
POA_API_KEY="secret123"  # per your instruction

# --- Load REDPILL_API_KEY from .env ---
if [[ -f .env ]]; then
  # shellcheck disable=SC2046
  export $(grep -E '^REDPILL_API_KEY=' .env | xargs) || true
fi
: "${REDPILL_API_KEY:?REDPILL_API_KEY not set in .env}"

# --- Random 400-char ASCII prompt ---
PROMPT="$(
  head -c 1024 /dev/urandom \
    | base64 \
    | tr -d '\n' \
    | tr -cd 'A-Za-z0-9 ' \
    | cut -c1-400
)"
if (( ${#PROMPT} < 400 )); then
  PROMPT="$(printf '%-400s' "$PROMPT" | tr ' ' 'x')"
fi

run_test() {
  local api_url=$1
  local api_key=$2
  local model=$3
  local times_file
  times_file=$(mktemp)

  echo "Testing model: $model at $api_url"

  for i in $(seq 1 "$ROUNDS"); do
    printf "Request %d... " "$i"
    duration=$(curl -sS -o /dev/null -w "%{time_total}\n" \
      -X POST "$api_url" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $api_key" \
      -d "{
        \"model\": \"$model\",
        \"messages\": [{\"role\": \"user\", \"content\": \"$PROMPT\"}],
        \"max_tokens\": 50
      }")
    echo "${duration}s"
    echo "$duration" >> "$times_file"
  done

  avg=$(awk '{s+=$1} END { if (NR>0) printf("%.3f", s/NR); else print "nan" }' "$times_file")
  echo "Average time for $model: ${avg}s"
  rm -f "$times_file"
  echo
}

# --- Benchmarks ---
# Red Pill
run_test "$REDPILL_URL" "$REDPILL_API_KEY" "mistralai/ministral-3b"
run_test "$REDPILL_URL" "$REDPILL_API_KEY" "mistralai/ministral-8b"

# Proof of Autonomy (hard-coded key)
run_test "$POA_URL" "$POA_API_KEY" "demo-gpt-4o-mini"