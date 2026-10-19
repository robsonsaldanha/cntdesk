#!/bin/bash
set -e

export PATH="$HOME/.cargo/bin:$PATH"

echo "=== CNT Desk Integration Tests ==="
echo "Running e2e tests..."

cargo test -p cntdesk-server --test e2e_test -- --nocapture

echo ""
echo "=== All integration tests passed ==="