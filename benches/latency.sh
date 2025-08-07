#!/usr/bin/env bash
set -euo pipefail

# --- Config ---
URL_HOST="notary.pse.dev"
URL_PORT=443
URL_PATH="/"  # Use root path
SAMPLES="${1:-10}"

echo "📈  Collecting ${SAMPLES} latency samples to root endpoint (/)..."
echo

# --- Main Loop ---
total_app_rtt=0

for i in $(seq 1 "${SAMPLES}"); do
    read pretransfer starttransfer <<<"$(
        curl --silent --output /dev/null \
             --no-keepalive --http1.1 \
             --head "https://${URL_HOST}:${URL_PORT}${URL_PATH}" \
             -w ' %{time_pretransfer} %{time_starttransfer}\n'
    )"

    # Calculate RTT after TLS is complete
    app_rtt=$(awk "BEGIN {print ${starttransfer} - ${pretransfer}}")

    app_ms=$(awk "BEGIN {printf \"%.2f\", ${app_rtt}*1000}")

    printf ' • run %2d → App-RTT %6.2f ms\n' \
        "$i" "$app_ms"

    total_app_rtt=$(awk "BEGIN {print ${total_app_rtt} + ${app_rtt}}")
done

avg_app=$(awk "BEGIN {print ${total_app_rtt}/${SAMPLES}}")

avg_app_ms=$(awk "BEGIN {printf \"%.2f\", ${avg_app}*1000}")

echo
echo "📊  Average Latency over ${SAMPLES} runs"
echo "    ▸ Application-layer RTT    : ${avg_app_ms} ms  ← (your target metric)"