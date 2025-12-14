#!/bin/bash

# Benchmark script for agent with different prover modes
# Runs each prover variation multiple times and reports average and std deviation

RUNS=10
PROVERS=("direct" "proxy" "tls-single")
BASE_CMD="./target/release/agent --polymarket-limit 3"

echo "Building release binary..."
cargo build --release -p agent || exit 1

echo ""
echo "Running benchmark: $RUNS iterations per prover"
echo "Provers: ${PROVERS[*]}"
echo "========================================"

# Arrays to store times for each prover
times_direct=()
times_proxy=()
times_tls_single=()
fails_direct=()
fails_proxy=()
fails_tls_single=()

for run in $(seq 1 $RUNS); do
    echo ""
    echo "Run $run/$RUNS"
    echo "----------------------------------------"

    for prover in "${PROVERS[@]}"; do
        echo -n "  $prover: "

        start=$(python3 -c 'import time; print(time.time())')
        $BASE_CMD --prover "$prover" > /dev/null 2>&1
        exit_code=$?
        end=$(python3 -c 'import time; print(time.time())')

        duration=$(awk "BEGIN {printf \"%.2f\", $end - $start}")

        if [ $exit_code -ne 0 ]; then
            echo "FAILED (exit code: $exit_code)"
            case $prover in
                direct) fails_direct+=($run) ;;
                proxy) fails_proxy+=($run) ;;
                tls-single) fails_tls_single+=($run) ;;
            esac
        else
            echo "${duration}s"
            case $prover in
                direct) times_direct+=($duration) ;;
                proxy) times_proxy+=($duration) ;;
                tls-single) times_tls_single+=($duration) ;;
            esac
        fi

        # 1 second between provers
        sleep 1
    done

    # 5 seconds between runs (except last)
    if [ $run -lt $RUNS ]; then
        echo "  (waiting 5s before next run...)"
        sleep 5
    fi
done

echo ""
echo "========================================"
echo "RESULTS"
echo "========================================"

# Function to calculate avg and std
calc_stats() {
    local name=$1
    shift
    local times=("$@")

    if [ ${#times[@]} -eq 0 ]; then
        echo "$name: No successful runs!"
        return
    fi

    local stats=$(printf '%s\n' "${times[@]}" | awk '
    {
        sum += $1
        sumsq += $1 * $1
        count++
    }
    END {
        avg = sum / count
        std = sqrt(sumsq / count - avg * avg)
        printf "%.2f %.2f %d", avg, std, count
    }')

    local avg=$(echo "$stats" | cut -d' ' -f1)
    local std=$(echo "$stats" | cut -d' ' -f2)
    local count=$(echo "$stats" | cut -d' ' -f3)

    echo "$name: avg=${avg}s std=${std}s (${count}/$RUNS successful)"
}

calc_stats "direct" "${times_direct[@]}"
calc_stats "proxy" "${times_proxy[@]}"
calc_stats "tls-single" "${times_tls_single[@]}"

# Report failures
echo ""
if [ ${#fails_direct[@]} -gt 0 ]; then
    echo "direct failures: runs ${fails_direct[*]}"
fi
if [ ${#fails_proxy[@]} -gt 0 ]; then
    echo "proxy failures: runs ${fails_proxy[*]}"
fi
if [ ${#fails_tls_single[@]} -gt 0 ]; then
    echo "tls-single failures: runs ${fails_tls_single[*]}"
fi