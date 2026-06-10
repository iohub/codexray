#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RUST_DIR="$SCRIPT_DIR/rust-core"
BIN_DIR="$HOME/.codexray/bin"
BIN_PATH="$BIN_DIR/codexray"

echo "==> [1/3] Building TypeScript wrapper..."
cd "$SCRIPT_DIR"
if [ -f "node_modules/.package-lock.json" ] || [ -d "node_modules/typescript" ]; then
    npx tsc 2>/dev/null && echo "    dist/ ready" || echo "    (tsc skipped)"
else
    echo "    (no node_modules, skipping TS build — only needed for npm publish)"
fi

echo ""
echo "==> [2/3] Building Rust binary..."
cd "$RUST_DIR"

MODE="${1:-}"
if [ "$MODE" = "--release" ] || [ "$MODE" = "-r" ]; then
    echo "    Building release..."
    cargo build --release
    RUST_BIN="$RUST_DIR/target/release/codexray"
else
    echo "    Building debug..."
    cargo build
    RUST_BIN="$RUST_DIR/target/debug/codexray"
fi

echo ""
echo "==> [3/3] Installing to $BIN_PATH"
mkdir -p "$BIN_DIR"
cp -f "$RUST_BIN" "$BIN_PATH"
chmod 755 "$BIN_PATH"

# Add to PATH if needed
if ! echo "$PATH" | tr ':' '\n' | grep -qF "$BIN_DIR"; then
    for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
        if [ -f "$rc" ]; then
            if ! grep -qF "$BIN_DIR" "$rc"; then
                echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$rc"
                echo "    Added $BIN_DIR to $rc"
            fi
        fi
    done
fi

BIN_SIZE=$(du -h "$BIN_PATH" | cut -f1)
echo ""
echo "==> Done! $BIN_PATH ($BIN_SIZE)"
echo "    Version: $($BIN_PATH --version 2>/dev/null || echo '...')"
echo ""
echo "    Usage:"
echo "      codexray init               # build index"
echo "      codexray search <query>     # semantic search"
echo "      codexray status             # index status"
echo "      codexray callers <symbol>   # find callers"
echo "      codexray callees <symbol>   # find callees"
echo "      codexray install            # register with Claude Code / Codex"
echo "      codexray serve --mcp        # run as MCP server"
echo "      codexray daemon             # start background watcher"
echo "      codexray daemon --background # fork to background"
echo ""
echo "    Tip: run 'source ~/.zshrc' if codexray not found"
