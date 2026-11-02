#!/usr/bin/env python3
"""assh 数据存储的 Python 参考实现（读 + 写）。

与 asshc/config.go、asshc/assh.go、asshc/utils.go、asshc/keygen/rsa.go、
kiris crypto.go / hash / file.go 的格式保持一致。用于：
  - 生成确定性测试 fixtures（绕开原版 B-004 写不截断缺陷）
  - 作为 Rust 数据层的交叉验证参照
"""
import hashlib
import json
import os
import shutil

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import rsa, padding
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes


def md5hex(b: bytes) -> bytes:
    return hashlib.md5(b).hexdigest().encode()


def _rsa_encrypt(pub_pem: bytes, data: bytes) -> bytes:
    pub = serialization.load_pem_public_key(pub_pem)
    return pub.encrypt(data, padding.PKCS1v15())


def _rsa_decrypt(priv_pem: bytes, ct: bytes) -> bytes:
    key = serialization.load_pem_private_key(priv_pem, password=None)
    return key.decrypt(ct, padding.PKCS1v15())


def _aes_encrypt(plain: bytes, key: bytes) -> bytes:
    pad = 16 - len(plain) % 16
    plain = plain + bytes([pad]) * pad
    enc = Cipher(algorithms.AES(key), modes.CBC(key[:16])).encryptor()
    return enc.update(plain) + enc.finalize()


def _aes_decrypt(ct: bytes, key: bytes) -> bytes:
    dec = Cipher(algorithms.AES(key), modes.CBC(key[:16])).decryptor()
    out = dec.update(ct) + dec.finalize()
    pad = out[-1]
    if pad < 1 or pad > 16 or out[-pad:] != bytes([pad]) * pad:
        raise ValueError("bad PKCS7 padding")
    return out[:-pad]


def gen_rsa_pem() -> tuple:
    """返回 (private_pem(PKCS1), public_pem(PKIX))，与 keygen.NewRsa 格式一致。"""
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    priv = key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.TraditionalOpenSSL,
        serialization.NoEncryption(),
    )
    pub = key.public_key().public_bytes(
        serialization.Encoding.PEM,
        serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return priv, pub


class Store:
    """一个 ~/.assh 目录。"""

    def __init__(self, basedir: str, master: str = None):
        self.basedir = os.path.abspath(basedir)
        self.priv = os.path.join(self.basedir, ".rsa")
        self.pub = os.path.join(self.basedir, ".rsa.pub")
        self.account = os.path.join(self.basedir, ".account")
        self.yaml = os.path.join(self.basedir, "assh.yml")
        self.db = os.path.join(self.basedir, "data", "servers.db")
        self.data = {}

    # ---- 生命周期 ----
    def create(self, master: str):
        os.makedirs(os.path.join(self.basedir, "data"), exist_ok=True)
        if not os.path.exists(self.priv):
            priv, pub = gen_rsa_pem()
            _write(self.priv, priv, 0o600)
            _write(self.pub, pub, 0o644)
        self.set_master(master)
        self.data = {}
        self.save_db()

    def set_master(self, master: str):
        password = hashlib.md5(master.encode()).hexdigest().encode()
        _write(self.account, _rsa_encrypt(_read(self.pub), password), 0o600)
        self._write_yaml()

    def _write_yaml(self):
        dbpath = getattr(self, "yaml_dbpath", self.basedir + "/data")
        yml = (
            "logPath: ~/.assh/assh.log\n"
            "logLevel: OFF\n"
            f"dbPath: {dbpath}\n"
            "qiniuAccessKey: \n"
            "qiniuSecretKey:\n"
            "qiniuBucket:\n"
        )
        _write(self.yaml, _rsa_encrypt(_read(self.pub), yml.encode()), 0o600)

    # ---- 读写 ----
    def master(self) -> bytes:
        return _rsa_decrypt(_read(self.priv), _read(self.account))  # MD5(主口令)

    def _write_yaml_plain(self) -> str:
        return _rsa_decrypt(_read(self.priv), _read(self.yaml)).decode()

    def load_db(self):
        password = self.master()
        blob = _read(self.db)
        ct = blob[64:]
        if md5hex(ct) != blob[32:64]:
            raise ValueError("servers.db 数据签名不匹配")
        if md5hex(password + ct) != blob[:32]:
            raise ValueError("servers.db 口令绑定校验失败")
        self.data = json.loads(_aes_decrypt(ct, password).decode())
        return self.data

    def save_db(self):
        password = self.master()
        plain = json.dumps(self.data, separators=(",", ":"), ensure_ascii=False).encode()
        ct = _aes_encrypt(plain, password)
        blob = md5hex(password + ct) + md5hex(ct) + ct
        _write(self.db, blob, 0o600)

    # ---- 服务器条目 ----
    @staticmethod
    def make_server(name, host, port, user, password="", key="", remark="", options=None):
        return {
            "name": name, "host": host, "port": port, "user": user,
            "password": password, "key": key, "remark": remark, "options": options,
        }

    def set_server(self, fullname: str, server: dict):
        group, sname = self.parse_name(fullname)
        self.data.setdefault(group, {})[sname] = server

    @staticmethod
    def parse_name(name: str):
        parts = name.split(".")
        if len(parts) < 2:
            return "", parts[0]
        return parts[0], parts[1]

    def put_key(self, group: str, filename: str, content: bytes):
        d = os.path.join(self.basedir, "data", group)
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, filename)
        _write(p, content, 0o600)
        return p


def _read(p: str) -> bytes:
    with open(p, "rb") as f:
        return f.read()


def _write(p: str, data: bytes, mode: int):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "wb") as f:
        f.write(data)
    os.chmod(p, mode)
