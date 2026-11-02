#!/bin/bash
# 生成测试 sshd host key（一次性，非真实凭据）。由 review.sh / Makefile 在缺失时调用。
set -euo pipefail
cd "$(dirname "$0")/.."
for d in hostkeys1 hostkeys2; do
  mkdir -p "tests/sshd/$d"
  [ -f "tests/sshd/$d/ssh_host_rsa_key" ] || ssh-keygen -q -t rsa -b 2048 -N '' -f "tests/sshd/$d/ssh_host_rsa_key"
  [ -f "tests/sshd/$d/ssh_host_ed25519_key" ] || ssh-keygen -q -t ed25519 -N '' -f "tests/sshd/$d/ssh_host_ed25519_key"
done
echo "hostkeys ready: $(ls tests/sshd/hostkeys1)"
