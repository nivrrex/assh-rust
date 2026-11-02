#!/bin/bash
# 下载官方预编译 oracle（v1.0.3 / v1.0.4）到 oracles/。
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p oracles/oracle-103 oracles/oracle-104 tmp
fetch() {
  local v=$1 d=$2 f="assh-linux-amd64_v${1}-${2}.zip"
  [ -x "oracles/oracle-${v##*.}/assh" ] && return
  [ -f "tmp/$f" ] || curl -sSL -o "tmp/$f" "https://github.com/keesely/assh/releases/download/v${v}/${f}"
  mkdir -p "tmp/x$v" && (cd "tmp/x$v" && unzip -oq "../$f")
  local out=$([ "$v" = "1.0.3" ] && echo oracle-103 || echo oracle-104)
  cp "tmp/x$v/assh-linux-amd64_v${v}-${d}/assh" "oracles/$out/assh"
  chmod +x "oracles/$out/assh"
}
fetch 1.0.3 20200328
fetch 1.0.4 20230322
echo "oracles ready:"; sha256sum oracles/oracle-*/assh
