#!/bin/bash
# Start automated benchmarks for AI Passport
#
# Usage: ./scripts/start-benchmark.sh
#
# Prerequisites:
#   - Infrastructure services running (see start-infra.sh)
#   - .env file configured with API credentials

set -e

echo "Starting AI Passport benchmarks..."

# Start benchmark in its own tmux session
tmux kill-session -t benchmark 2>/dev/null || true
tmux new-session -d -s benchmark "RUST_LOG=automated=info cargo run --bin automated-benchmarks --release"

echo "Benchmark started in tmux session:"
echo "  - benchmark: tmux attach -t benchmark"
echo ""
echo "To list sessions: tmux ls"

# Attach to the benchmark session
tmux attach -t benchmark
