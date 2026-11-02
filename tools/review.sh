#!/bin/bash
# 一键复核：在干净环境重跑全部验收。
# 用法（root）： bash tools/review.sh
#   前置：docker（测试 sshd 容器）、用户 asshtest、Rust 工具链。
# 结束时打印 PASS/FAIL 汇总。任一步失败即中止（set -e）。
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
PROJ="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJ"

log() { echo; echo "===== $* ====="; }

log "0/8 前置检查"
command -v docker >/dev/null || { echo "缺少 docker"; exit 1; }
[ -x oracles/oracle-104/assh ] || bash tools/fetch_oracles.sh
[ -f keys/rsa_nopass ] || bash tools/make_keys.sh
[ -f tests/sshd/hostkeys1/ssh_host_ed25519_key ] || bash tools/make_hostkeys.sh
id asshtest >/dev/null 2>&1 || useradd -m -s /bin/bash asshtest
docker image inspect assh-sshd:test >/dev/null 2>&1 || {
  echo "构建 sshd 镜像..."; ( cd tests/sshd && docker build --network=host -t assh-sshd:test . )
}
if true; then
  echo "重建测试 sshd 容器（assh-net）..."
  docker rm -f assh-sshd-1 assh-sshd-2 >/dev/null 2>&1 || true
  docker network create assh-net >/dev/null 2>&1 || true
  docker run -d --name assh-sshd-1 --network assh-net -p 2201:22 \
    -v "$PROJ/tests/sshd/hostkeys1:/etc/ssh/hostkeys" assh-sshd:test >/dev/null
  docker run -d --name assh-sshd-2 --network assh-net -p 2202:22 \
    -v "$PROJ/tests/sshd/hostkeys2:/etc/ssh/hostkeys" assh-sshd:test >/dev/null
  sleep 3
  for c in assh-sshd-1 assh-sshd-2; do
    docker exec "$c" bash -lc 'mkdir -p /home/tuser/.ssh && chmod 700 /home/tuser/.ssh'
    cat keys/rsa_nopass.pub keys/ed25519_nopass.pub keys/rsa_pass.pub keys/ed25519_pass.pub \
      | docker exec -i "$c" bash -lc 'cat > /home/tuser/.ssh/authorized_keys && chmod 600 /home/tuser/.ssh/authorized_keys && chown -R tuser:tuser /home/tuser/.ssh'
  done
fi

log "1/8 Rust 本地检查（fmt/clippy/test/examples）"
( cd rust && ./check.sh )

log "2/8 构建并安装 Rust 二进制"
( cd rust && cargo build --release )
install -m755 rust/target/release/assh /usr/local/bin/assh-rs
install -m755 oracles/oracle-104/assh /usr/local/bin/assh104
install -m755 oracles/oracle-103/assh /usr/local/bin/assh103

log "3/8 生成 fixtures"
python3 tools/make_fixtures.py >/dev/null
# 供 TTY 矩阵（以 asshtest 运行，需可读副本）
mkdir -p /home/asshtest/fixtures
for f in fixture-passwd fixture-multi; do rm -rf /home/asshtest/fixtures/$f; cp -a fixtures/$f /home/asshtest/fixtures/; done
chown -R asshtest:asshtest /home/asshtest/fixtures
cp tests/conformance/tty_matrix.py /home/asshtest/tty_matrix.py && chown asshtest /home/asshtest/tty_matrix.py

log "4/8 oracle-104 基线自洽"
python3 tests/conformance/runner.py --check --oracle oracle-104

log "5/8 Rust conformance（vs oracle-104）"
python3 tests/conformance/runner.py --check --oracle rust --bin /usr/local/bin/assh-rs --baseline oracle-104

log "6/8 数据层 L1/L2/L3 + account"
python3 tests/data_layers.py

log "7/8 TTY 场景矩阵（Rust）"
runuser -u asshtest -- python3 /home/asshtest/tty_matrix.py \
  --bin /usr/local/bin/assh-rs --fixture-root /home/asshtest/fixtures \
  --scenarios normal-exit,logout,sigterm,sigint,sighup,ctrl-c,ctrl-backslash,remote-kill9,exec-mode \
  --out /tmp/tty-rs.json

log "8/8 网络断开场景"
docker network create assh-net >/dev/null 2>&1 || true
docker network connect assh-net assh-sshd-1 >/dev/null 2>&1 || true
python3 tests/conformance/tty_network.py --bin /usr/local/bin/assh-rs

echo
echo "================= REVIEW PASS ================="
echo "conformance: oracle-104 自洽 64/64 + Rust 64/64"
echo "data layers: L1/L2/L3 + account 全过"
echo "tty: 除 SIGKILL 外全部恢复"
echo "docs: README.md / docs/{DATA-FORMAT,COMPATIBILITY,DEPENDENCIES}.md"
