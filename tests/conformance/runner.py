#!/usr/bin/env python3
"""assh conformance 套件：录制/校验 oracle-103 / oracle-104 / Rust 版行为。

用法：
  runner.py --record --oracle oracle-104
  runner.py --check  --oracle oracle-104
  runner.py --check  --oracle rust --bin /path/to/assh-rs

设计：
  - fixture 安装到测试用户 home（assh 用 os/user 取 home，忽略 $HOME）
  - 非交互：subprocess + runuser
  - 交互：pexpect 驱动 runuser
  - golden 存 tests/conformance/golden/<oracle>/<case-id>.json
  - 规范化：时间、进度、Go map 随机顺序、路径
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys

PROJ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FIXROOT = os.path.join(PROJ, "fixtures")
GOLDEN = os.path.join(PROJ, "tests", "conformance", "golden")
TESTUSER = "asshtest"
TESTHOME = "/home/asshtest"
TARGET = f"{TESTHOME}/.assh"

ORACLES = {
    "oracle-103": "/usr/local/bin/assh103",
    "oracle-104": "/usr/local/bin/assh104",
}

# stdout+stderr 合并顺序具竞态的用例（结果载荷按行排序后比较）
SORT_RESULT_CASES = {"c-stdout-stderr", "direct-c-stderr"}
# 输出行顺序由 Go map 随机决定、内容无顺序语义的用例
SORT_ALL_CASES = {"rm-group", "ping-multi", "bc-group"}

# 已登记的原版缺陷修复：这些用例的对应字段差异是预期的（见 DECISIONS BD-00x）
INTENTIONAL_DIFF = {
    "c-exit7": ["rc"],           # BD-004：Rust 回传远端退出码
    "direct-c-exit7": ["rc"],
    "badpw-exec": ["rc", "stdout", "stderr"],   # BD-003/B-011：Rust 不 panic，报认证失败
    "nouser-exec": ["rc", "stdout", "stderr"],
    "direct-c-badpw": ["rc", "stdout", "stderr"],
    "mv-missing": ["rc", "stderr"],    # B-013：Rust 不 panic
    "direct-login-pty": ["stdout"],          # B-003：oracle 回显 -P 口令，Rust 不回显
    # 原版将错误静默吞掉（B-005），Rust 返回非零并写 stderr；stdout 已对齐
    "login-unknown": ["rc", "stderr"],
    "unreachable-c": ["rc", "stderr"],
    "push-missing": ["rc", "stderr"],
    "pull-missing": ["rc", "stderr"],
    "push-explicit-remote": ["rc", "stderr"],
    # push/pull 目标不存在时 oracle 会交互式问口令（Rust 不实现该提示）
    "push-badserver": ["rc", "stdout", "stderr"],
    # ping 的 stdout 进度行（Scanning: n/n）未复刻；结果行在 stderr 已对齐
    "ping-multi": ["stdout"],
    # Rust 版帮助/版本：只列已实现命令、含 --strict，version=v1.0.4-compat（有意不同）
    "help-main": ["stdout"],
    "help-login": ["stdout"],
    "version": ["stdout"],
}


# ---------------- 环境安装 ----------------
def install_fixture(name):
    src = os.path.join(FIXROOT, name, ".assh")
    if not os.path.isdir(src):
        raise SystemExit(f"fixture not found: {name}; run tools/make_fixtures.py")
    if os.path.exists(TARGET):
        shutil.rmtree(TARGET)
    shutil.copytree(src, TARGET)
    subprocess.run(["chown", "-R", f"{TESTUSER}:{TESTUSER}", TARGET], check=True)


def fixture_db():
    """返回当前安装 fixture 的 servers.db 解密后的规范化 JSON。"""
    sys.path.insert(0, os.path.join(PROJ, "tools"))
    from assh_store import Store
    st = Store(TARGET)
    try:
        data = st.load_db()
    except Exception as e:  # 原版可能损坏
        return {"__error__": str(e)}
    return normalize_data(data)


def normalize_data(data):
    """去掉 map 顺序影响：按 group/server 名排序。"""
    out = {}
    for g in sorted(data):
        out[g] = {n: data[g][n] for n in sorted(data[g])}
    return out


def db_sha256():
    import hashlib
    p = os.path.join(TARGET, "data", "servers.db")
    if not os.path.exists(p):
        return None
    return hashlib.sha256(open(p, "rb").read()).hexdigest()


# ---------------- 规范化 ----------------
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def normalize_out(s, fixture, binname=None, sort_result=False, sort_all=False):
    s = s.replace("\r\n", "\n")
    # 仅规范已知二进制名，避免误伤 asshtest / assh-rust 等
    s = re.sub(r"\b(assh103|assh104|assh-rs|assh_rs)\b", "<BIN>", s)
    # 原版 B-003：`-P` 明文回显；Rust 不回显 → 从基线中去除
    s = re.sub(r"Password: [^\s>]+", "", s)
    # 时间戳/耗时
    s = re.sub(r"time cost: [0-9.]+(ms|s|µs|us|ns)", "time cost: <T>", s)
    s = re.sub(r"\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2}", "<TS>", s)
    s = re.sub(r"Scanning: \d+/\d+", "Scanning: <N>", s)
    s = re.sub(r"Last login: .*", "Last login: <TS>", s)
    # 容器主机名（重建后变化）
    s = re.sub(r"tuser@[0-9a-f]{12}", "tuser@<HOST>", s)
    s = re.sub(r"0x[0-9a-f]+", "0xADDR", s)
    # push 每文件 time cost 前的空行：oracle 由进度行 \r 吸收，Rust 无进度行 → 统一折叠
    s = s.replace("\n\n time cost:", "\n time cost:")
    if sort_all:
        s = "\n".join(sorted(l for l in s.split("\n") if l))
    # sftp 进度行（\r 覆盖型）直接丢弃
    lines = [l for l in s.split("\n") if not re.match(r"\s*(upload|download):", l)]
    s = "\n".join(lines)
    s = s.replace(TARGET, "<ASSDIR>")
    s = s.replace(TESTHOME, "<HOME>")
    # Go map 随机顺序：仅对 ls/search/ping 表格按行排序
    if ("Group Name" in s or "Server Name" in s) and s.count("\n> ") > 1:
        lines = s.split("\n")
        head = [l for l in lines if not l.startswith("> ")]
        body = sorted(l for l in lines if l.startswith("> "))
        s = "\n".join(body + head) if body else s
    if sort_result:
        s = _sort_result_payload(s)
    return s


def _sort_result_payload(s):
    """`-c` 的 stdout+stderr 合并顺序是竞态；对 Result 载荷行排序后再比。"""
    marker = "> Result ("
    i = s.find(marker)
    if i < 0:
        return s
    head_end = s.find("\n", i)
    if head_end < 0:
        return s
    head_end += 1
    head, rest = s[:head_end], s[head_end:]
    tc = rest.find("\n\n>  time cost")
    payload, tail = (rest[:tc], rest[tc:]) if tc >= 0 else (rest, "")
    trailing = "\n" if payload.endswith("\n") else ""
    lines = payload.rstrip("\n").split("\n")
    return head + "\n".join(sorted(lines)) + trailing + tail


def stable(s):
    # 完全去掉颜色，便于比对（颜色保留会因 map 顺序/终端差异噪声大）
    return ANSI.sub("", s or "")


# ---------------- 执行 ----------------
def run_cmd(binary, argv, stdin=None, timeout=30, cwd="/tmp"):
    cmd = ["runuser", "-u", TESTUSER, "--", "env",
           "PATH=/usr/local/bin:/usr/bin:/bin", "TERM=xterm-256color",
           binary] + argv
    # 关键：绝不继承调用者的终端 stdin。
    # 原版 push/pull 在“未知服务器且无口令”时会 fmt.Scanln 等待口令
    # （cmd/server.go:350-351）——若继承交互 tty 会挂起直至超时；
    # 固定 DEVNULL 后该路径恒为 EOF，与 golden 锁定一致。
    if stdin is None:
        p = subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, timeout=timeout, cwd=cwd)
    else:
        p = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, timeout=timeout, cwd=cwd)
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def run_pty(binary, argv, send, timeout=30, cwd="/tmp"):
    """pexpect 交互：send 是 [(expect_or_None, reply_or_None, delay)] 步骤列表。"""
    import pexpect
    cmd = (f"runuser -u {TESTUSER} -- env PATH=/usr/local/bin:/usr/bin:/bin "
           f"TERM=xterm-256color {binary} " + " ".join(argv))
    c = pexpect.spawn(cmd, timeout=timeout, encoding="utf-8", cwd=cwd,
                      dimensions=(24, 80))
    log = []
    try:
        for exp, reply, delay in send:
            if exp:
                c.expect(exp)
                log.append(c.before + (c.after or ""))
            if reply is not None:
                c.send(reply)
            if delay:
                import time
                time.sleep(delay)
        c.expect(pexpect.EOF)
        log.append(c.before)
    except Exception as e:
        log.append(f"<EXC {type(e).__name__}: {e}>")
    return c.exitstatus, "".join(log), ""


# ---------------- 用例定义 ----------------
def C(cid, fixture, argv, stdin=None, timeout=30, pty=None):
    return dict(id=cid, fixture=fixture, argv=argv, stdin=stdin,
                timeout=timeout, pty=pty)


def cases():
    out = []
    # version / help
    out.append(C("version", "fixture-empty", ["version"]))
    out.append(C("help-main", "fixture-empty", ["--help"]))
    out.append(C("help-login", "fixture-empty", ["login", "--help"]))
    # ls / search / info
    out.append(C("ls-empty", "fixture-empty", ["ls"]))
    out.append(C("ls-multi", "fixture-multi", ["ls"]))
    out.append(C("ls-passwd", "fixture-passwd", ["ls"]))
    out.append(C("search-hit", "fixture-multi", ["search", "srv-a"]))
    out.append(C("search-miss", "fixture-multi", ["search", "zzz"]))
    out.append(C("info-ok", "fixture-passwd", ["info", "srv-pass"]))
    out.append(C("info-missing", "fixture-passwd", ["info", "nope"]))
    # set / rm / mv
    out.append(C("set-new", "fixture-passwd",
                 ["set", "srv-new", "-H", "127.0.0.1", "-p", "2201", "-l", "tuser",
                  "-P", "Pa+ss_w0rd", "-R", "new", "-f"]))
    out.append(C("set-confirm-no", "fixture-passwd",
                 ["set", "srv-pass", "-H", "127.0.0.1", "-p", "2201", "-l", "tuser",
                  "-P", "Pa+ss_w0rd", "-R", "overwrite"], stdin=b"n\n"))
    out.append(C("set-confirm-yes", "fixture-passwd",
                 ["set", "srv-pass", "-H", "127.0.0.1", "-p", "2202", "-l", "tuser",
                  "-P", "Pa+ss_w0rd", "-R", "overwritten", "-f"]))
    out.append(C("set-p-zero", "fixture-passwd",
                 ["set", "srv-pass", "-p", "0", "-R", "p0", "-f"]))
    out.append(C("rm-force", "fixture-multi", ["rm", "-f", "srv-b"]))
    out.append(C("rm-confirm-no", "fixture-multi", ["rm", "srv-a"], stdin=b"n\n"))
    out.append(C("rm-confirm-yes", "fixture-multi", ["rm", "srv-a"], stdin=b"y\n"))
    out.append(C("rm-missing", "fixture-multi", ["rm", "-f", "nope"]))
    out.append(C("rm-group", "fixture-multi", ["rm", "-f", "-g", "grp"]))
    out.append(C("mv-ok", "fixture-multi", ["mv", "srv-a", "renamed"]))
    out.append(C("mv-exists", "fixture-multi", ["mv", "srv-a", "srv-b"]))
    out.append(C("mv-missing", "fixture-multi", ["mv", "nope", "x"]))
    # export / import
    out.append(C("export-default", "fixture-multi", ["export", "/tmp/cf-export.yml"]))
    out.append(C("export-empty", "fixture-empty", ["export", "/tmp/cf-export-e.yml"]))
    out.append(C("import-file", "fixture-empty", ["import", "/tmp/cf-import.yml", "-f"]))
    # 错误路径
    out.append(C("login-unknown", "fixture-empty", ["login", "no-such-server"], timeout=15))
    out.append(C("info-badpw", "fixture-passwd", ["info", "srv-pass"]))
    out.append(C("push-missing", "fixture-passwd", ["push", "srv-pass", "/no/such/file"]))
    out.append(C("pull-missing", "fixture-passwd", ["pull", "srv-pass", "/no/such/file"]))
    out.append(C("push-badserver", "fixture-empty", ["push", "ghost", "/etc/hostname"], timeout=10))
    # -c 远程命令
    out.append(C("c-single", "fixture-passwd", ["srv-pass", "-c", "echo HELLO"]))
    out.append(C("c-semi", "fixture-passwd", ["srv-pass", "-c", "echo A; echo B"]))
    out.append(C("c-quote", "fixture-passwd", ["srv-pass", "-c", "echo 'q u o'"]))
    out.append(C("c-stdout-stderr", "fixture-passwd",
                 ["srv-pass", "-c", "echo OUT; echo ERR 1>&2"]))
    out.append(C("c-exit7", "fixture-passwd", ["srv-pass", "-c", "exit 7"]))
    out.append(C("c-exit0", "fixture-passwd", ["srv-pass", "-c", "true"]))
    out.append(C("c-whoami", "fixture-passwd", ["srv-pass", "-c", "id -un"]))
    # 认证方式
    out.append(C("key-exec", "fixture-key", ["srv-key", "-c", "echo KEYOK"]))
    out.append(C("key-ed-exec", "fixture-key", ["srv-ed", "-c", "echo EDOK"]))
    out.append(C("keyp-exec", "fixture-key-pass", ["srv-keyp", "-c", "echo KEYPOK"]))
    out.append(C("keyp-ed-exec", "fixture-key-pass", ["srv-edp", "-c", "echo EDPOK"]))
    # 不可达
    out.append(C("unreachable-c", "fixture-unreachable", ["srv-down", "-c", "echo x"], timeout=20))
    # 认证错误
    out.append(C("badpw-exec", "fixture-badpw", ["srv-badpw", "-c", "echo x"], timeout=20))
    out.append(C("nouser-exec", "fixture-badpw", ["srv-nouser", "-c", "echo x"], timeout=20))
    # 引号嵌套
    out.append(C("c-nested", "fixture-passwd", ["srv-pass", "-c", 'echo "a;b" ; echo c']))
    # 直连（不依赖存储）：-H/-p/-l/-P
    DC = ["-H", "127.0.0.1", "-p", "2201", "-l", "tuser", "-P", "Pa+ss_w0rd"]
    out.append(C("direct-c-single", "fixture-empty", DC + ["-c", "echo D1"]))
    out.append(C("direct-c-semi", "fixture-empty", DC + ["-c", "echo A; echo B"]))
    out.append(C("direct-c-stderr", "fixture-empty", DC + ["-c", "echo OUT; echo ERR 1>&2"]))
    out.append(C("direct-c-exit7", "fixture-empty", DC + ["-c", "exit 7"]))
    out.append(C("direct-c-exit0", "fixture-empty", DC + ["-c", "true"]))
    out.append(C("direct-c-badpw", "fixture-empty",
                 ["-H", "127.0.0.1", "-p", "2201", "-l", "tuser", "-P", "NOPE", "-c", "echo x"], timeout=20))
    out.append(C("direct-login-pty", "fixture-empty", DC, timeout=10,
                 pty=[(None, None, 1.0), ("[$#]", "echo DPTY\r", 0.5), (None, None, 0.3), ("[$#]", "exit\r", 0.5)]))
    # push / pull 功能路径
    out.append(C("push-file", "fixture-passwd", ["push", "srv-pass", "/tmp/cf-file.txt"]))
    out.append(C("push-explicit-remote", "fixture-passwd",
                 ["push", "srv-pass", "/tmp/cf-file.txt", "/home/tuser/cf-up.txt"]))
    out.append(C("push-dir", "fixture-passwd", ["push", "srv-pass", "/tmp/cf-dir"]))
    out.append(C("push-b", "fixture-passwd", ["push", "srv-pass", "-b", "4096", "/tmp/cf-file.txt"]))
    out.append(C("pull-file", "fixture-passwd",
                 ["pull", "srv-pass", "/home/tuser/cf-remote.txt", "/tmp/cf-pull/"]))
    out.append(C("pull-dir", "fixture-passwd",
                 ["pull", "srv-pass", "/home/tuser/cf-remotedir", "/tmp/cf-pull/"]))
    # P2: ping / bc
    out.append(C("ping-empty", "fixture-empty", ["ping"]))
    out.append(C("ping-multi", "fixture-multi", ["ping"], timeout=40))
    out.append(C("bc-group", "fixture-multi", ["bc", "grp", "echo BATCH"], timeout=40))
    out.append(C("bc-missing", "fixture-multi", ["bc", "nope", "echo X"], timeout=20))
    # 交互（pexpect）
    out.append(C("pty-login-exit", "fixture-passwd", ["srv-pass"], timeout=10,
                 pty=[(None, None, 1.0), ("[$#]", "echo PTYOK\r", 0.5), (None, None, 0.3), ("[$#]", "exit\r", 0.5)]))
    out.append(C("pty-ctrlc", "fixture-passwd", ["srv-pass"], timeout=10,
                 pty=[(None, None, 1.0), ("[$#]", "sleep 30\r", 0.5),
                      (None, "\x03", 0.5), ("[$#]", "echo AFTER\r", 0.5),
                      (None, None, 0.3), ("[$#]", "exit\r", 0.5)]))
    return out


# ---------------- 录制/校验 ----------------
def execute(binary, case):
    install_fixture(case["fixture"])
    db_before = json.dumps(fixture_db(), sort_keys=True, ensure_ascii=False)
    if case["pty"] is not None:
        rc, out, err = run_pty(binary, case["argv"], case["pty"], case["timeout"])
    else:
        rc, out, err = run_cmd(binary, case["argv"], case["stdin"], case["timeout"])
    db_after = json.dumps(fixture_db(), sort_keys=True, ensure_ascii=False)
    return {
        "argv": case["argv"],
        "rc": rc,
        "stdout": normalize_out(stable(out), case["fixture"], os.path.basename(binary),
                                     case["id"] in SORT_RESULT_CASES,
                                     case["id"] in SORT_ALL_CASES),
        "stderr": normalize_out(stable(err), case["fixture"], os.path.basename(binary),
                                     False, case["id"] in SORT_ALL_CASES),
        "db_before": db_before,
        "db_after": db_after,
        "db_changed": db_before != db_after,
    }


def prep_remote():
    script = (
        "printf 'remote constancy\\n%.0s' $(seq 1 200) > /home/tuser/cf-remote.txt; "
        "mkdir -p /home/tuser/cf-remotedir; echo dirA > /home/tuser/cf-remotedir/a; "
        "echo dirB > /home/tuser/cf-remotedir/b; "
        "rm -f /home/tuser/cf-file.txt /home/tuser/cf-up.txt; "
        "rm -rf /home/tuser/cf-dir; "
        "chown -R tuser:tuser /home/tuser"
    )
    for cont in ("assh-sshd-1", "assh-sshd-2"):
        subprocess.run(["docker", "exec", cont, "bash", "-lc", script], check=False)


def prep_local():
    os.makedirs("/tmp/cf-dir/sub", exist_ok=True)
    with open("/tmp/cf-file.txt", "w") as f:
        f.write("hello constancy\n" * 200)
    with open("/tmp/cf-dir/a.txt", "w") as f:
        f.write("A" * 100)
    with open("/tmp/cf-dir/sub/b.txt", "w") as f:
        f.write("B" * 100)
    shutil.rmtree("/tmp/cf-pull", ignore_errors=True)
    os.makedirs("/tmp/cf-pull")
    os.chmod("/tmp/cf-pull", 0o777)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--record", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--oracle", required=True)
    ap.add_argument("--bin", default=None, help="Rust 二进制路径")
    ap.add_argument("--baseline", default=None, help="校验时比对的 golden 目录（默认同 --oracle）")
    ap.add_argument("--only", default=None, help="只跑匹配的 case id 前缀")
    args = ap.parse_args()

    binary = args.bin or ORACLES.get(args.oracle)
    if not binary:
        raise SystemExit(f"unknown oracle {args.oracle} and no --bin")
    outdir = os.path.join(GOLDEN, args.baseline or args.oracle)
    if args.record:
        os.makedirs(os.path.join(GOLDEN, args.oracle), exist_ok=True)
        outdir = os.path.join(GOLDEN, args.oracle)

    all_cases = cases()
    if args.only:
        all_cases = [c for c in all_cases if c["id"].startswith(args.only)]

    prep_local()
    prep_remote()

    # 准备 import/export 辅助文件
    with open("/tmp/cf-import.yml", "w") as f:
        f.write("nil:\n  imp1:\n    name: imp1\n    host: 127.0.0.1\n"
                "    port: 2201\n    user: tuser\n    password: Pa+ss_w0rd\n    remark: imported\n")

    npass = nfail = 0
    for case in all_cases:
        try:
            got = execute(binary, case)
        except Exception as e:
            print(f"[ERR ] {case['id']}: {type(e).__name__}: {e}")
            nfail += 1
            continue
        gpath = os.path.join(outdir, case["id"] + ".json")
        if args.record:
            with open(gpath, "w") as f:
                json.dump(got, f, indent=2, ensure_ascii=False)
            print(f"[REC ] {case['id']}: rc={got['rc']} db_changed={got['db_changed']}")
            npass += 1
        else:
            if not os.path.exists(gpath):
                print(f"[MISS] {case['id']}: no golden")
                nfail += 1
                continue
            want = json.load(open(gpath))
            diffs = [k for k in ("rc", "stdout", "stderr", "db_changed")
                     if want.get(k) != got.get(k)]
            diffs = [d for d in diffs if d not in INTENTIONAL_DIFF.get(case["id"], [])]
            if diffs:
                nfail += 1
                print(f"[FAIL] {case['id']}: {diffs}")
                for k in diffs:
                    w, g = want.get(k), got.get(k)
                    if isinstance(w, str):
                        import difflib
                        d = "\n".join(difflib.unified_diff(w.split("\n"), g.split("\n"),
                                                           "oracle", "rust", lineterm="", n=1))
                        print("   " + d.replace("\n", "\n   ")[:1500])
                    else:
                        print(f"   {k}: oracle={w!r} rust={g!r}")
            else:
                npass += 1
                print(f"[ OK ] {case['id']}")
    print(f"\n{args.oracle}: {npass} passed, {nfail} failed")
    return 1 if nfail else 0


if __name__ == "__main__":
    sys.exit(main())
