#!/bin/bash
# 生成测试密钥对（RSA/Ed25519 × 有/无 passphrase）。测试专用，非真实凭据。
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p keys && cd keys
gen() {
  if [ -f "$1" ]; then return 0; fi
  local bits=()
  # 仅当指定了位数才传 -b（ed25519 不接受 -b，空值会被 OpenSSH 9.2 拒绝）
  if [ -n "${4:-}" ]; then bits=(-b "$4"); fi
  ssh-keygen -q -t "$2" "${bits[@]}" -N "$3" -C "test-$1" -f "$1"
}
gen rsa_nopass rsa "" 2048
gen rsa_pass rsa "keypass+_123" 2048
gen ed25519_nopass ed25519 ""
gen ed25519_pass ed25519 "keypass+_123"
echo "keys ready: $(ls)"
