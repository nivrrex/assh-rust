# assh-rust 构建与测试入口
PROJ := $(shell pwd)
ORACLE ?= oracle-104
PY := python3

.PHONY: help fixtures conformance conformance-record conformance-103 conformance-rs data-layers tty-matrix tty-network sshd-up sshd-down sshd-build clean-rust

RUST_BIN ?= rust/target/release/assh

help:
	@echo "make fixtures            # 生成/重建 fixtures"
	@echo "make conformance         # 对 $(ORACLE) 校验 golden"
	@echo "make conformance-record  # 录制 $(ORACLE) golden"
	@echo "make conformance-103     # 校验 oracle-103"
	@echo "make tty-baseline        # 复现 TTY 未恢复（(需 fixture-passwd 已安装)"
	@echo "make sshd-up / sshd-down # 测试 sshd 容器"

fixtures:
	$(PY) tools/make_fixtures.py

conformance: fixtures
	$(PY) tests/conformance/runner.py --check --oracle $(ORACLE)

conformance-record: fixtures
	$(PY) tests/conformance/runner.py --record --oracle $(ORACLE)

conformance-103: fixtures
	$(PY) tests/conformance/runner.py --check --oracle oracle-103

conformance-rs: fixtures
	$(PY) tests/conformance/runner.py --check --oracle rust --bin $(RUST_BIN) --baseline oracle-104

data-layers: fixtures
	$(PY) tests/data_layers.py

tty-matrix:
	$(PY) tests/conformance/tty_matrix.py --bin $(RUST_BIN) --fixture-root /home/asshtest/fixtures

tty-network:
	$(PY) tests/conformance/tty_network.py --bin $(RUST_BIN)

tty-baseline:
	$(PY) tests/conformance/tty_baseline.py --bin /usr/local/bin/assh104

sshd-build:
	cd tests/sshd && docker build --network=host -t assh-sshd:test .

sshd-up:
	bash tools/make_hostkeys.sh
	docker run -d --name assh-sshd-1 -p 2201:22 -v $(PROJ)/tests/sshd/hostkeys1:/etc/ssh/hostkeys assh-sshd:test
	docker run -d --name assh-sshd-2 -p 2202:22 -v $(PROJ)/tests/sshd/hostkeys2:/etc/ssh/hostkeys assh-sshd:test
	$(MAKE) sshd-keys

sshd-keys:
	for c in assh-sshd-1 assh-sshd-2; do \
	  docker exec $$c bash -lc 'mkdir -p /home/tuser/.ssh && chmod 700 /home/tuser/.ssh'; \
	  cat keys/rsa_nopass.pub keys/ed25519_nopass.pub | docker exec -i $$c bash -lc 'cat > /home/tuser/.ssh/authorized_keys && chmod 600 /home/tuser/.ssh/authorized_keys && chown -R tuser:tuser /home/tuser/.ssh'; \
	done

sshd-down:
	-docker rm -f assh-sshd-1 assh-sshd-2
