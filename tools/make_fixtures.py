#!/usr/bin/env python3
"""生成 conformance 测试用的确定性 fixtures。

每个 fixture 是 `fixtures/<name>/` 目录，内含完整 `~/.assh` 内容；
并生成 `fixtures/<name>/build.sh` 记录如何重放（调用本脚本）。
master 口令统一为 `Master+Pass_1`（含特殊字符）。
"""
import os
import shutil
import subprocess
import sys

PROJ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(PROJ, "tools"))
from assh_store import Store  # noqa: E402

MASTER = "Master+Pass_1"
KEYS = os.path.join(PROJ, "keys")
FIXROOT = os.path.join(PROJ, "fixtures")
TEST_HOME = "/home/asshtest"


def key_path(group, fname):
    return f"{TEST_HOME}/.assh/data/{group}/{fname}"


def build(name, fn):
    d = os.path.join(FIXROOT, name)
    if os.path.exists(d):
        shutil.rmtree(d)
    os.makedirs(d)
    st = Store(os.path.join(d, ".assh"), MASTER)
    st.yaml_dbpath = f"{TEST_HOME}/.assh/data"
    st.create(MASTER)
    fn(st)
    st.save_db()
    # 记录重放脚本
    with open(os.path.join(d, "build.sh"), "w") as f:
        f.write(f"#!/bin/bash\n# 重放：python3 tools/make_fixtures.py {name}\nexec python3 \"$(dirname \"$0\")/../../tools/make_fixtures.py\" {name}\n")
    os.chmod(os.path.join(d, "build.sh"), 0o755)
    return d


def srv(st, fullname, host, port, user, password="", keyfile=None, keydir="", remark="", options=None):
    key = ""
    if keyfile:
        short = st.parse_name(fullname)[1]
        kd = keydir or short
        content = open(os.path.join(KEYS, keyfile), "rb").read()
        st.put_key(kd, keyfile, content)
        key = f"{TEST_HOME}/.assh/data/{kd}/{keyfile}"
    st.set_server(fullname, Store.make_server(
        st.parse_name(fullname)[1], host, port, user, password, key, remark, options))


def f_empty(st):
    pass


def f_passwd(st):
    srv(st, "srv-pass", "127.0.0.1", 2201, "tuser", "Pa+ss_w0rd", remark="passwd auth")


def f_special(st):
    srv(st, "srv-special", "127.0.0.1", 2201, "tuser", "Pa+ss_w0rd!@#$%^&*()_+-=", remark="special")


def f_key(st):
    srv(st, "srv-key", "127.0.0.1", 2201, "tuser", "", keyfile="rsa_nopass", remark="key nopass")
    srv(st, "srv-ed", "127.0.0.1", 2201, "tuser", "", keyfile="ed25519_nopass", remark="ed nopass")


def f_key_pass(st):
    # getAuth 用 server.Password 作为私钥 passphrase
    srv(st, "srv-keyp", "127.0.0.1", 2201, "tuser", "keypass+_123", keyfile="rsa_pass", remark="key with pass")
    srv(st, "srv-edp", "127.0.0.1", 2201, "tuser", "keypass+_123", keyfile="ed25519_pass", remark="ed with pass")


def f_remark(st):
    srv(st, "srv-remark", "127.0.0.1", 2201, "tuser", "Pa+ss_w0rd", remark="héllo 备注 with spaces")


def f_multi(st):
    srv(st, "srv-a", "127.0.0.1", 2201, "tuser", "Pa+ss_w0rd", remark="a")
    srv(st, "srv-b", "127.0.0.1", 2202, "tuser", "Pa+ss_w0rd", remark="b")
    srv(st, "grp.g1", "127.0.0.1", 2201, "tuser", "Pa+ss_w0rd", remark="g1")
    srv(st, "grp.g2", "127.0.0.1", 2202, "tuser", "Pa+ss_w0rd", remark="g2")


def f_unreachable(st):
    srv(st, "srv-down", "127.0.0.1", 2299, "tuser", "Pa+ss_w0rd", remark="closed port")
    srv(st, "srv-unknown", "no.such.host.invalid", 22, "root", "x", remark="dns fail")


def f_badpw(st):
    srv(st, "srv-badpw", "127.0.0.1", 2201, "tuser", "WRONG-password", remark="bad password")
    srv(st, "srv-nouser", "127.0.0.1", 2201, "ghost", "Pa+ss_w0rd", remark="bad user")


BUILDERS = {
    "fixture-empty": f_empty,
    "fixture-passwd": f_passwd,
    "fixture-special": f_special,
    "fixture-key": f_key,
    "fixture-key-pass": f_key_pass,
    "fixture-remark": f_remark,
    "fixture-multi": f_multi,
    "fixture-unreachable": f_unreachable,
    "fixture-badpw": f_badpw,
}


def main():
    # 测试密钥缺失时自动生成（见 tools/make_keys.sh）
    need = ["rsa_nopass", "rsa_pass", "ed25519_nopass", "ed25519_pass"]
    if not all(os.path.exists(os.path.join(KEYS, k)) for k in need):
        subprocess.run(["bash", os.path.join(PROJ, "tools", "make_keys.sh")], check=True)
    names = sys.argv[1:] or list(BUILDERS)
    for n in names:
        if n == "fixture-real":
            continue
        d = build(n, BUILDERS[n])
        # 自检：能被本库读回
        st = Store(os.path.join(d, ".assh"), MASTER)
        st.load_db()
        print(f"built {n}: groups={ {g: list(v) for g, v in st.data.items()} }")
    # fixture-real：真实库只读副本（仅本机开发时有；公开克隆不含）
    real = os.path.join(FIXROOT, "fixture-real")
    src = next(
        (p for p in (
            os.path.join(PROJ, "artifacts", "real-db-backup"),
            os.path.join(PROJ, "internal-notes", "artifacts", "real-db-backup"),
        ) if os.path.isdir(p)),
        None,
    )
    if src:
        if os.path.exists(real):
            shutil.rmtree(real)
        os.makedirs(real)
        shutil.copytree(src, os.path.join(real, ".assh"))
        # 只重写 yaml 中的 dbPath，指向测试 home；不动 servers.db 与 RSA/account
        st = Store(os.path.join(real, ".assh"))
        st.yaml_dbpath = f"{TEST_HOME}/.assh/data"
        st._write_yaml()
        with open(os.path.join(real, "build.sh"), "w") as f:
            f.write("#!/bin/bash\n# 真实库只读副本（仅本机开发时有），禁止写入。\nexit 0\n")
        os.chmod(os.path.join(real, "build.sh"), 0o755)
        print("built fixture-real (read-only copy)")
    else:
        print("skip fixture-real (无 real-db-backup，仅本机开发需要)")


if __name__ == "__main__":
    main()
