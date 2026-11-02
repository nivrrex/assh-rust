#!/bin/bash
# 测试环境隔离：assh 用 os/user 取 home（忽略 $HOME），故需专用用户。
# 可用环境变量覆盖：ASSH_TEST_USER / ASSH_TEST_HOME / ASSH_PROJ
TESTUSER="${ASSH_TEST_USER:-asshtest}"
TESTHOME="${ASSH_TEST_HOME:-/home/$TESTUSER}"
PROJ="${ASSH_PROJ:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
# run_as_test <cmd...>：在测试用户下运行
run_as_test() { runuser -u "$TESTUSER" -- "$@"; }
