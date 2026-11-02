#!/bin/bash
# 构建并把 Rust 二进制装到测试用户可读位置（/root 不可遍历）
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/../rust"
cargo build
install -m755 target/debug/assh /usr/local/bin/assh-rs
echo "installed /usr/local/bin/assh-rs"
