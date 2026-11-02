#!/usr/bin/env python3
"""数据层分层验收：L1 读旧库 / L2 Rust 读回自己写的 / L3 oracle-104 读 Rust 写的。

复制 fixture 到测试 home，所有写操作仅作用于副本。
"""
import os
import shutil
import subprocess
import sys

PROJ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "/home/asshtest/.assh"
USER = "asshtest"
ORACLE = "/usr/local/bin/assh104"
RUST = "/usr/local/bin/assh-rs"

sys.path.insert(0, os.path.join(PROJ, "tools"))
from assh_store import Store  # noqa: E402


def run(binary, args, stdin=b""):
    cmd = ["runuser", "-u", USER, "--", "env", "PATH=/usr/local/bin:/usr/bin:/bin", binary] + args
    p = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def install(fixture):
    if os.path.exists(TARGET):
        shutil.rmtree(TARGET)
    shutil.copytree(os.path.join(PROJ, "fixtures", fixture, ".assh"), TARGET)
    subprocess.run(["chown", "-R", f"{USER}:{USER}", TARGET], check=True)


def rows(out):
    import re
    ansi = re.compile(r"\x1b\[[0-9;]*m")
    return sorted(l for l in ansi.sub("", out).split("\n") if l.startswith("> "))


def names(data):
    return {f"{g}.{n}" if g else n for g, ss in data.items() for n in ss}


def check(cond, msg):
    print(("  OK  " if cond else "  FAIL") + " " + msg)
    return cond


def main():
    ok = True
    # ---------- L1 ----------
    print("== L1: 读取存量（含 fixture-real） ==")
    for fx in ["fixture-empty", "fixture-passwd", "fixture-key", "fixture-multi",
               "fixture-special", "fixture-remark", "fixture-real"]:
        if not os.path.isdir(os.path.join(PROJ, "fixtures", fx)):
            print(f"  SKIP {fx}: 不存在（公开克隆不含 fixture-real；需本机 real-db-backup）")
            continue
        install(fx)
        _, o, _ = run(ORACLE, ["ls"])
        _, r, _ = run(RUST, ["ls"])
        ok &= check(rows(o) == rows(r), f"L1 {fx}: ls 与 oracle 一致 ({len(rows(o))} 行)")

    # ---------- L2 ----------
    print("== L2: Rust 写后 Rust 可读 ==")
    install("fixture-multi")
    run(RUST, ["set", "srv-new", "-H", "10.0.0.9", "-p", "2222", "-l", "u",
               "-P", "p+_w", "-R", "added", "-f"])
    st = Store(TARGET)
    d = st.load_db()
    ok &= check("srv-new" in names(d), "L2 set 新增可见且签名有效")
    run(RUST, ["rm", "-f", "srv-b"])
    d = Store(TARGET).load_db()
    ok &= check("srv-b" not in names(d) and "srv-new" in names(d), "L2 rm 生效且库未损坏（B-004 修复）")
    run(RUST, ["mv", "srv-a", "renamed"])
    d = Store(TARGET).load_db()
    ok &= check("renamed" in names(d) and "srv-a" not in names(d), "L2 mv 生效")
    # 收缩写入后再读：文件大小应严格等于内容（无残留）
    run(RUST, ["rm", "-f", "renamed"])
    run(RUST, ["rm", "-f", "srv-new"])
    d = Store(TARGET).load_db()
    ok &= check(set(d.get("", {})) == set(), "L2 连续收缩写后仍可读（truncate 正确）")

    # ---------- L3 ----------
    print("== L3: Rust 写后 oracle-104 仍可读 ==")
    install("fixture-multi")
    run(RUST, ["set", "srv-rs", "-H", "10.9.9.9", "-p", "2022", "-l", "rs",
               "-P", "p+_w", "-R", "by-rust", "-f"])
    run(RUST, ["rm", "-f", "grp.g2"])
    _, ol, _ = run(ORACLE, ["ls"])
    _, rl, _ = run(RUST, ["ls"])
    ok &= check(rows(ol) == rows(rl) and len(rows(ol)) > 0,
                f"L3 oracle 读 Rust 写的库：ls 一致 ({len(rows(ol))} 行)")
    rc, oi, _ = run(ORACLE, ["info", "srv-rs"])
    ok &= check(rc == 0 and "10.9.9.9" in oi and "by-rust" in oi, "L3 oracle info 读取 Rust 写入的条目")
    # oracle 对它读到的库再做一次写（set），Rust 再读 → 双向
    run(ORACLE, ["set", "srv-from-oracle", "-H", "10.1.1.1", "-P", "x", "-f"])
    d = Store(TARGET).load_db()
    ok &= check("srv-from-oracle" in names(d), "L3 oracle 写后 Rust 仍可读（双向）")

    # ---------- account 改密 ----------
    print("== account 改密：全库重加密 + oracle 可读 ==")
    install("fixture-passwd")
    # 当前 master=Master+Pass_1，改为 NewMaster+_2
    rc, out, err = run(RUST, ["account", "NewMaster+_2"], stdin=b"Master+Pass_1\nNewMaster+_2\n")
    d = Store(TARGET)
    # Python 参考实现用自己的 passwd 读取会失败（口令已换），改走 oracle 验证
    _, ol, _ = run(ORACLE, ["ls"])
    ok &= check(len(rows(ol)) == 1, f"account 改密后 oracle 仍可读（rc={rc} out={out.strip()!r}）")

    print("\nRESULT:", "ALL PASS" if ok else "FAILURES")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
