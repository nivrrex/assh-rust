#!/usr/bin/env python3
"""独立解密 assh 数据（不依赖 assh 源码），用于验证 AUDIT.md 中的数据格式报告。

格式（来源 asshc/config.go, asshc/assh.go, asshc/utils.go, asshc/keygen/rsa.go,
kiris crypto.go）：
  ~/.assh/.rsa        PKCS#1 RSA-2048 私钥 (PEM "RSA PRIVATE KEY")
  ~/.assh/.account    RSA_PKCS1v15( MD5(主口令) )            -> 32 字节 ASCII
  ~/.assh/assh.yml    RSA_PKCS1v15( YAML 文本 )
  ~/.assh/data/servers.db
      [0:32]   hex(MD5( passwd + ciphertext ))
      [32:64]  hex(MD5( ciphertext ))
      [64:]    ciphertext = AES-256-CBC(PKCS7( JSON ), key=passwd, iv=passwd[:16])
      passwd = MD5(主口令) 的 32 字符十六进制小写串（即 .account 解密结果）

用法：
  decrypt_assh.py [--basedir ~/.assh] [--redact|--raw]
默认 --redact：密码字段被替换为 ***，仅用于格式核验。
"""
import argparse, base64, hashlib, json, os, sys

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import padding


def rsa_decrypt(private_pem: bytes, ciphertext: bytes) -> bytes:
    key = serialization.load_pem_private_key(private_pem, password=None)
    return key.decrypt(ciphertext, padding.PKCS1v15())


def rsa_encrypt(public_pem: bytes, plaintext: bytes) -> bytes:
    pub = serialization.load_pem_public_key(public_pem)
    return pub.encrypt(plaintext, padding.PKCS1v15())


def aes_cbc_decrypt(ciphertext: bytes, key: bytes) -> bytes:
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    if len(key) not in (16, 24, 32):
        raise ValueError(f"bad AES key length {len(key)}")
    dec = Cipher(algorithms.AES(key), modes.CBC(key[:16])).decryptor()
    out = dec.update(ciphertext) + dec.finalize()
    # PKCS7 unpad
    pad = out[-1]
    if pad < 1 or pad > 16 or out[-pad:] != bytes([pad]) * pad:
        raise ValueError("bad PKCS7 padding")
    return out[:-pad]


def aes_cbc_encrypt(plaintext: bytes, key: bytes) -> bytes:
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    pad = 16 - len(plaintext) % 16
    plaintext = plaintext + bytes([pad]) * pad
    enc = Cipher(algorithms.AES(key), modes.CBC(key[:16])).encryptor()
    return enc.update(plaintext) + enc.finalize()


def md5hex(data: bytes) -> bytes:
    return hashlib.md5(data).hexdigest().encode()


def read_passwd(basedir: str) -> bytes:
    priv = open(os.path.join(basedir, ".rsa"), "rb").read()
    account = open(os.path.join(basedir, ".account"), "rb").read()
    return rsa_decrypt(priv, account)  # MD5(主口令) 的十六进制串


def decrypt_servers_db(basedir: str, passwd: bytes) -> str:
    blob = open(os.path.join(basedir, "data", "servers.db"), "rb").read()
    head_pw, head_data, ct = blob[:32], blob[32:64], blob[64:]
    if md5hex(ct) != head_data:
        raise ValueError("data MD5 signature mismatch")
    if md5hex(passwd + ct) != head_pw:
        raise ValueError("password-bound MD5 mismatch")
    return aes_cbc_decrypt(ct, passwd).decode()


def encrypt_servers_db(basedir: str, passwd: bytes, plaintext: str) -> bytes:
    ct = aes_cbc_encrypt(plaintext.encode(), passwd)
    return md5hex(passwd + ct) + md5hex(ct) + ct


def decrypt_yaml(basedir: str) -> str:
    priv = open(os.path.join(basedir, ".rsa"), "rb").read()
    return rsa_decrypt(priv, open(os.path.join(basedir, "assh.yml"), "rb").read()).decode()


def redact(obj):
    if isinstance(obj, dict):
        return {k: ("***" if k.lower() in ("password", "passwd") else redact(v))
                for k, v in obj.items()}
    if isinstance(obj, list):
        return [redact(x) for x in obj]
    return obj


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--basedir", required=True)
    ap.add_argument("--raw", action="store_true", help="不脱敏（含真实密码）")
    ap.add_argument("--yaml", action="store_true", help="同时打印解密后的配置 YAML")
    args = ap.parse_args()

    pw = read_passwd(args.basedir)
    print(f"# passwd(=MD5(master)): {'*'*len(pw)} (len={len(pw)})", file=sys.stderr)
    plain = decrypt_servers_db(args.basedir, pw)
    data = json.loads(plain)
    print(json.dumps(data if args.raw else redact(data), indent=2, ensure_ascii=False))
    if args.yaml:
        print("--- assh.yml ---")
        print(decrypt_yaml(args.basedir))


if __name__ == "__main__":
    main()
