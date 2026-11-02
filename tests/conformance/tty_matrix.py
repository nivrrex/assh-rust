#!/usr/bin/env python3
"""Phase 7 TTY 场景矩阵：验证异常退出后终端 termios 已恢复。

以 asshtest 运行：
  runuser -u asshtest -- python3 tests/conformance/tty_matrix.py --bin /usr/local/bin/assh-rs --fixture fixture-passwd

每个场景验收 = 退出后 pty termios 与新建 pty 默认值一致（即回到 cooked 模式）。
"""
import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import termios
import time
import pty

import pexpect

PROJ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TARGET = "/home/asshtest/.assh"
FIXROOT = os.path.join(PROJ, "fixtures")
FLAGS_KEYS = ["ICRNL", "IXON", "ISIG", "ICANON", "ECHO", "IEXTEN", "OPOST", "ONLCR", "ECHOCTL"]
_BITS = {k: getattr(termios, k) for k in FLAGS_KEYS if hasattr(termios, k)}
_LFLAG = {"ISIG", "ICANON", "ECHO", "IEXTEN", "ECHOCTL"}


def read_flags(fd):
    a = termios.tcgetattr(fd)
    out = {}
    for k, bit in _BITS.items():
        src = a[3] if k in _LFLAG else a[0] if k in ("ICRNL", "IXON") else a[1]
        out[k] = bool(src & bit)
    return out


def reference():
    m, s = pty.openpty()
    f = read_flags(s)
    os.close(m)
    os.close(s)
    return f


def install(fixture, fixroot):
    if os.path.exists(TARGET):
        shutil.rmtree(TARGET)
    shutil.copytree(os.path.join(fixroot, fixture, ".assh"), TARGET)
    subprocess.run(["chown", "-R", "asshtest:asshtest", TARGET], check=True)


def spawn(binary, args):
    cmd = f"bash -c 'exec {binary} " + " ".join(args) + "'"
    return pexpect.spawn(cmd, timeout=20, encoding="utf-8", dimensions=(24, 80))


def scenario(binary, name):
    if name == "normal-exit":
        c = spawn(binary, ["srv-pass"])
        c.expect(r"[\$#] ")
        c.sendline("exit")
        c.expect(pexpect.EOF)
    elif name == "logout":
        c = spawn(binary, ["srv-pass"])
        c.expect(r"[\$#] ")
        c.sendline("logout")
        c.expect(pexpect.EOF)
    elif name in ("sigterm", "sigint", "sighup", "sigkill"):
        c = spawn(binary, ["srv-pass"])
        c.expect(r"[\$#] ")
        sig = {"sigterm": signal.SIGTERM, "sigint": signal.SIGINT,
               "sighup": signal.SIGHUP, "sigkill": signal.SIGKILL}[name]
        os.kill(c.pid, sig)
        try:
            c.expect(pexpect.EOF, timeout=10)
        except Exception:
            c.close(force=True)
    elif name == "ctrl-c":
        c = spawn(binary, ["srv-pass"])
        c.expect(r"[\$#] ")
        c.send("\x03")           # 转发到远端
        time.sleep(0.5)
        c.sendline("echo AFTER_CTRLC")
        c.expect("AFTER_CTRLC")
        c.sendline("exit")
        c.expect(pexpect.EOF)
    elif name == "ctrl-backslash":
        c = spawn(binary, ["srv-pass"])
        c.expect(r"[\$#] ")
        c.send("\x1c")           # 远端 SIGQUIT
        time.sleep(0.5)
        c.sendline("exit")
        try:
            c.expect(pexpect.EOF, timeout=8)
        except Exception:
            c.close(force=True)
    elif name == "remote-kill9":
        c = spawn(binary, ["srv-pass"])
        c.expect(r"[\$#] ")
        c.sendline("kill -9 $$")
        try:
            c.expect(pexpect.EOF, timeout=10)
        except Exception:
            c.close(force=True)
    elif name == "exec-mode":
        c = pexpect.spawn(f"{binary} srv-pass -c 'echo EXECMODE'", timeout=15,
                          encoding="utf-8")
        c.expect(pexpect.EOF)
    else:
        raise SystemExit("unknown scenario " + name)
    return read_flags(c.child_fd)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--fixture", default="fixture-passwd")
    ap.add_argument("--fixture-root", default=FIXROOT)
    ap.add_argument("--out", default=None)
    ap.add_argument("--scenarios", default="normal-exit,logout,sigterm,sigint,sighup,ctrl-c,ctrl-backslash,remote-kill9,exec-mode,sigkill")
    args = ap.parse_args()

    install(args.fixture, args.fixture_root)
    ref = reference()
    results = []
    for name in args.scenarios.split(","):
        try:
            post = scenario(args.bin, name)
            d = {k: (ref[k], post[k]) for k in ref if ref[k] != post[k]}
        except Exception as e:
            d = {"__error__": f"{type(e).__name__}: {e}"}
        results.append({"scenario": name, "diff": d})
        mark = "OK " if not d else "BAD"
        print(f"[{mark}] {name}: {d if d else 'restored'}")
    if args.out:
        with open(args.out, "w") as f:
            json.dump(results, f, indent=2)
    bad = [r["scenario"] for r in results if r["diff"]]
    print(f"\n未恢复场景: {bad or '无'}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
