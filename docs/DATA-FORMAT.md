# 数据格式（`~/.assh`）

assh-rs 与上游 assh v1.0.3 / v1.0.4 使用**相同的数据布局**，可直接读写既有数据。

## 目录布局

```
~/.assh/
├── .rsa            PKCS#1 RSA-2048 私钥（PEM: RSA PRIVATE KEY）
├── .rsa.pub        PKIX 公钥（PEM: PUBLIC KEY）
├── .account        RSA_PKCS1v15( MD5(主口令) )
├── assh.yml        RSA_PKCS1v15( YAML 配置：logPath/logLevel/dbPath/qiniu* )
└── data/
    ├── servers.db  服务器清单（加密）
    └── <server>/   该服务器使用的私钥副本（若配置了 -k）
```

## `servers.db` 布局

```
[ 0:32)   hex(MD5( passwd || ciphertext ))      口令绑定校验
[32:64)   hex(MD5( ciphertext ))                数据完整性校验
[64:  )   ciphertext = AES-256-CBC( PKCS7(JSON) )
          key = passwd（32 字节）
          iv  = passwd[0:16]
```

- `passwd` = `MD5(主口令)` 的 32 字符小写十六进制串（由 `.account` 解出）。
- 明文 JSON：`map[group]map[name]Server`，`Server` 字段：
  `name, host, port, user, password, key, remark, options`。
- 无分组服务器归入组名 `""`。

## 口令与配置

- `.account` 与 `assh.yml` 用 `~/.assh/.rsa.pub` 做 **RSA PKCS#1 v1.5** 加密。
- 主口令**仅**在 `account` 命令中交互使用；其余命令从 `.account` 经 `.rsa` 解出
  `MD5(主口令)`，**不提示输入**。
- `set` 中 `password` 字段同时用作私钥的 passphrase（与上游一致）。

## 兼容性

- **读**：可读上游 v1.0.3 / v1.0.4 生成的 `servers.db`（含分组、备注、密钥路径）。
- **写**：assh-rs 采用**原子写**（临时文件 + rename），写出的库上游 v1.0.4 仍可读（双向兼容）。
- ⚠️ 上游 v1.0.4 存在“写入不截断”缺陷：库变小（如 `rm`/`mv`）时可能残留旧尾部导致库损坏。
  在两者并用时，**写操作建议用 assh-rs**。

## 导出 / 导入（明文）

`export` / `import` 使用**明文 YAML**（含口令与私钥文本），请妥善保管：

```yaml
nil:                 # 无分组服务器
  myserver:
    name: myserver
    host: 127.0.0.1
    port: 22
    user: root
    password: ...
    remark: ...
    pemkey: {}       # 或 { private: ..., public: ... }
mygroup:             # 分组服务器
  srv2: { ... }
```

## 首次运行初始化

`~/.assh` 不存在时，**任何需要数据层的命令**（`account`/`ls`/`set`/`login` …）会自动初始化，
对齐上游 `config.go` 的 `init()`（幂等，已存在的文件不改动）：

1. 创建 `~/.assh/`；
2. 若 `.rsa` 与 `.rsa.pub` **都不存在**，生成 RSA-2048 密钥对（`.rsa` 权限 `0600`）；
3. 若 `assh.yml` 不存在，用 `.rsa.pub` 加密写入默认配置（`dbPath` 为绝对路径）；
4. 创建 `dbPath` 目录（默认 `~/.assh/data`）。

- `.account` 与 `servers.db` **不在初始化中创建**：首次 `assh account` 设置主口令后才写 `.account`；
  首次设置**不产生** `servers.db`（与上游一致）。
- `-v` / `--version` / `--help` **不触发**初始化（无文件副作用）。
- `.rsa`/`.rsa.pub` 若被手工建成了目录，会给出可操作错误提示。
- `servers.db` 存在但 `.account` 缺失时，报错提示先执行 `assh account`（不静默呈现空库）。
