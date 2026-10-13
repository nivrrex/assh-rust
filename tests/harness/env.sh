#!/bin/bash
# 测试环境隔离：assh 用 os/user 取 home（忽略 $HOME），故用专用用户 asshtest。
TESTUSER=asshtest
TESTHOME=/home/asshtest
PROJ=/root/Desktop/assh-rust
# run_oracle <oracle-dir> <args...>：在测试用户下运行 oracle
run_as_test() { runuser -u "$TESTUSER" -- "$@"; }
