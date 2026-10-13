#!/usr/bin/env python3
"""TTY 反面基线：复现 oracle 在异常退出时未恢复终端模式。

必须在 asshtest 用户下运行（assh 用 os/user 取 home）：
    runuser -u asshtest -- python3 tests/conformance/tty_baseline.py --bin /usr/local/bin/assh104

原理：
  - pexpect 分配一个 pty；`bash -c 'exec <assh> <server>'` 使 assh 成为该 pty 的前台进程，
    且 c.pid 就是 assh 的 pid（bash exec 不换 pid）。
  - 参照 termios 取一个新建 pty 的默认值（Linux 上所有新建 pty 默认一致）。
  - assh 进入 raw 后向其发送 SIGTERM/SIGINT/SIGHUP/SIGKILL，再读 pty 的 termios，
    与参照比较 → 差异即未恢复的 termios 标志位。
  - 正常 exit 作为对照。
"""
import argparse
import json
import os
import signal
import sys
import termios
import pty

import pexpect

FLAG_NAMES = [
    "IGNBRK", "BRKINT", "IGNPAR", "PARMRK", "INPCK", "ISTRIP", "INLCR", "IGNCR",
    "ICRNL", "IUCLC", "IXON", "IXANY", "IXOFF", "IMAXBEL", "IUTF8",
    "OPOST", "OLCUC", "ONLCR", "OCRNL", "ONOCR", "ONLRET", "OFILL", "OFDEL",
    "ISIG", "ICANON", "XCASE", "ECHO", "ECHOE", "ECHOK", "ECHONL", "NOFLSH",
    "TOSTOP", "IEXTEN", "ECHOCTL", "ECHOKE", "PENDIN",
]
# 构造 flag -> bit 映射
_BITS = {}
for name in FLAG_NAMES:
    if hasattr(termios, name):
        _BITS[name] = getattr(termios, name)


def flags_of(fd):
    attrs = termios.tcgetattr(fd)
    lflag, iflag = attrs[3], attrs[0]
    out = {}
    for name, bit in _BITS.items():
        val = lflag if name in ("ISIG", "ICANON", "XCASE", "ECHO", "ECHOE", "ECHOK",
                                "ECHONL", "NOFLSH", "TOSTOP", "IEXTEN", "ECHOCTL",
                                "ECHOKE", "PENDIN") else iflag
        out[name] = bool(val & bit)
    return out


def reference_flags():
    m, s = pty.openpty()
    f = flags_of(s)
    os.close(m)
    os.close(s)
    return f


def diff(before, after):
    return {k: (before[k], after[k]) for k in before if before[k] != after[k]}


def scenario(binary, server, action, timeout=15):
    child = pexpect.spawn(f"bash -c 'exec {binary} {server}'", timeout=timeout,
                          encoding="utf-8", dimensions=(24, 80))
    raw_seen = False
    try:
        child.expect("connection:")
        # 等待进入 raw
        child.expect([pexpect.TIMEOUT, pexpect.EOF], timeout=2)
        raw_seen = True
    except Exception as e:
        child.close(force=True)
        return {"action": action, "error": f"{type(e).__name__}: {e}", "raw_seen": False,
                "diff": {}, "post": {}, "ref": {}}

    during = flags_of(child.child_fd)
    if action == "normal":
        child.sendline("exit")
        try:
            child.expect(pexpect.EOF, timeout=timeout)
        except Exception:
            pass
    else:
        sig = {"SIGTERM": signal.SIGTERM, "SIGINT": signal.SIGINT,
               "SIGHUP": signal.SIGHUP, "SIGKILL": signal.SIGKILL}[action]
        os.kill(child.pid, sig)
        try:
            child.expect(pexpect.EOF, timeout=timeout)
        except Exception:
            child.close(force=True)
    post = flags_of(child.child_fd)
    ref = reference_flags()
    return {
        "action": action,
        "raw_seen": raw_seen,
        "during_raw": {k: v for k, v in during.items() if not v},
        "ref": ref,
        "post": post,
        "diff": diff(ref, post),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--server", default="srv-pass")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    results = []
    for action in ("normal", "SIGTERM", "SIGINT", "SIGHUP", "SIGKILL"):
        r = scenario(args.bin, args.server, action)
        results.append(r)
        d = r.get("diff", {})
        print(f"[{action:8}] raw_seen={r.get('raw_seen')} diff={d}")
        if r.get("error"):
            print("        error:", r["error"])
    if args.out:
        with open(args.out, "w") as f:
            json.dump(results, f, indent=2)
    bad = [r for r in results if r.get("diff")]
    print(f"\n未恢复终端的场景: {[r['action'] for r in bad] or '无'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
