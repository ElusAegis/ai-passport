#!/bin/bash
# Start infrastructure services for AI Passport development
#
# Usage: ./scripts/start-infra.sh
#
# This script starts the following services in separate tmux sessions:
#   - notary: Local TLSNotary server
#   - model-server: Mock model server for testing
#   - proxy-server: Attestation proxy server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

echo "Starting AI Passport infrastructure..."

# Start notary in its own tmux session
tmux kill-session -t notary 2>/dev/null || true
tmux new-session -d -s notary "cargo run -p notary --release"
echo -e "${GREEN}✓${NC} Started notary server"

# Start model-server in its own tmux session
tmux kill-session -t model-server 2>/dev/null || true
tmux new-session -d -s model-server "source .env 2>/dev/null; cargo run -p model-server --release"
echo -e "${GREEN}✓${NC} Started model server"

# Start proxy-server in its own tmux session
tmux kill-session -t proxy-server 2>/dev/null || true
tmux new-session -d -s proxy-server "source .env 2>/dev/null; RUST_LOG=proxy_server=info cargo run -p proxy-server --release"
echo -e "${GREEN}✓${NC} Started proxy server"

echo ""
echo "Services started in tmux sessions:"
echo "  - notary:       tmux attach -t notary"
echo "  - model-server: tmux attach -t model-server"
echo "  - proxy-server: tmux attach -t proxy-server"
echo ""
echo "To list sessions: tmux ls"
echo "To stop all:      tmux kill-server"
