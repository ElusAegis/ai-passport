#!/bin/bash

# Start notary in its own tmux session
tmux kill-session -t notary 2>/dev/null
tmux new-session -d -s notary "source ~/.cargo/env && cd ~/ai-passport && cargo run -p notary --release"

# Start model-server in its own tmux session
tmux kill-session -t model-server 2>/dev/null
tmux new-session -d -s model-server "source ~/.cargo/env && cd ~/ai-passport && source .env && cargo run -p model-server --release"

# Start proxy-server in its own tmux session
tmux kill-session -t proxy-server 2>/dev/null
tmux new-session -d -s proxy-server "source ~/.cargo/env && cd ~/ai-passport && source .env && RUST_LOG=proxy_server=info cargo run -p proxy-server --release"

echo "Services started in tmux sessions:"
echo "  - notary: tmux attach -t notary"
echo "  - model-server: tmux attach -t model-server"
echo "  - proxy-server: tmux attach -t proxy-server"
echo ""
echo "To list sessions: tmux ls"
