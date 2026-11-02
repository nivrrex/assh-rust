#!/usr/bin/env python3
"""Phase 7 网络异常场景（需 root 以操作 docker）：
  - 本地网络断开：连接建立后 docker network disconnect，assh 应退出并恢复终端
用法（root）：python3 tests/conformance/tty_network.py --bin /usr/local/bin/assh-rs
"""
import argparse
import os
import shutil
import subprocess
import sys
import termios
import pty

import pexpect

PROJ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIXROOT = os.path.join(PROJ, "fixtures")
TARGET = "/home/asshtest/.assh"
NET = "assh-net"
CONT = "assh-sshd-1"

KEYS = ["ICRNL", "IXON", "ISIG", "ICANON", "ECHO", "IEXTEN", "OPOST", "ONLCR"]
_L = {"ISIG", "ICANON", "ECHO", "IEXTEN"}


def read_flags(fd):
    a = termios.tcgetattr(fd)
    return {k: bool((a[3] if k in _L else a[0] if k in ("ICRNL", "IXON") else a[1]) & getattr(termios, k))
            for k in KEYS}


def reference():
    m, s = pty.openpty()
    f = read_flags(s)
    os.close(m)
    os.close(s)
    return f


def install(fixture):
    if os.path.exists(TARGET):
        shutil.rmtree(TARGET)
    shutil.copytree(os.path.join(FIXROOT, fixture, ".assh"), TARGET)
    subprocess.run(["chown", "-R", "asshtest:asshtest", TARGET], check=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--fixture", default="fixture-passwd")
    args = ap.parse_args()
    install(args.fixture)
    ref = reference()

    cmd = (f"runuser -u asshtest -- env PATH=/usr/local/bin:/usr/bin:/bin "
           f"TERM=xterm-256color {args.bin} srv-pass")
    c = pexpect.spawn(cmd, timeout=30, encoding="utf-8", dimensions=(24, 80))
    c.expect(r"[\$#] ")
    print("connected; disconnecting network...")
    subprocess.run(["docker", "network", "disconnect", NET, CONT], check=False)
    try:
        c.expect(pexpect.EOF, timeout=25)
        exited = True
    except Exception as e:
        exited = False
        print("did not exit:", type(e).__name__)
        c.close(force=True)
    post = read_flags(c.child_fd)
    subprocess.run(["docker", "network", "connect", NET, CONT], check=False)
    diff = {k: (ref[k], post[k]) for k in ref if ref[k] != post[k]}
    print(f"exited={exited} termios_diff={diff or 'restored'}")
    return 0 if exited and not diff else 1


if __name__ == "__main__":
    sys.exit(main())
