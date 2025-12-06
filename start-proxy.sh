#!/bin/bash

# Start proxy-server in its own tmux session
tmux kill-session -t proxy-server 2>/dev/null
tmux new-session -d -s proxy-server "source ~/.cargo/env && cd ~/ai-passport && source .env && RUST_LOG=proxy_server=info cargo run -p proxy-server --release"

echo "Proxy server started in tmux session:"
echo "  - proxy-server: tmux attach -t proxy-server"
echo ""
echo "To list sessions: tmux ls"