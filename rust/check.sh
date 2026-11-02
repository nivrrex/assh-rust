#!/bin/bash
# Rust 侧本地检查：格式 / clippy / 单元测试 / examples 编译
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")"
echo "== cargo fmt --check =="
cargo fmt --check
echo "== cargo clippy =="
cargo clippy --all-targets -- -D warnings
echo "== cargo test =="
cargo test
echo "== cargo build --examples =="
cargo build --examples
echo "OK"
