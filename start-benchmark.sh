#!/bin/bash

# Start benchmark in its own tmux session
tmux kill-session -t benchmark 2>/dev/null
tmux new-session -d -s benchmark "source ~/.cargo/env && cd ~/ai-passport && RUST_LOG=automated=info cargo run --bin automated-benchmarks --release"

echo "Benchmark started in tmux session:"
echo "  - benchmark: tmux attach -t benchmark"
echo ""
echo "To list sessions: tmux ls"

tmux attach -t benchmark

