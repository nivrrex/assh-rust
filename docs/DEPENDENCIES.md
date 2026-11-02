# 依赖与安全

## 工具链

| 项 | 版本 |
|---|---|
| rustc / cargo | 1.99.0（stable） |
| edition | 2024 |
| 锁定文件 | `rust/Cargo.lock`（已入库） |

## 直接依赖（`Cargo.lock` 锁定）

| crate | 版本 | License |
|---|---|---|
| aes | 0.9.3 | MIT OR Apache-2.0 |
| anyhow | 1.0.104 | MIT OR Apache-2.0 |
| cbc | 0.2.1 | MIT OR Apache-2.0 |
| libc | 0.2.190 | MIT OR Apache-2.0 |
| md-5 | 0.11.0 | MIT OR Apache-2.0 |
| nix | 0.31.3 | MIT |
| rand | 0.10.3 | MIT OR Apache-2.0 |
| rpassword | 7.5.4 | Apache-2.0 |
| rsa | 0.9.10 | MIT OR Apache-2.0 |
| russh | 0.64.1 | Apache-2.0 |
| russh-sftp | 3.0.1 | Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| serde_yaml | 0.9.34+deprecated | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| tokio | 1.53.2 | MIT |

传递依赖中另有 `rsa 0.10.0-rc.18`、`rand 0.8.8`（由 russh 引入）。全部为宽松许可
（MIT / Apache-2.0），无 GPL。

## 安全审计（`cargo audit`）

```
error: 2 vulnerabilities found!
warning: 1 allowed warning found
```

| ID | crate | 严重度 | 说明 | 处置 |
|---|---|---|---|---|
| RUSTSEC-2023-0071 | rsa 0.9.10 / 0.10.0-rc.18 | 5.9 medium（Marvin Attack） | RSA 解密时序侧信道；**无修复版本** | 接受：RSA 仅用于**本地**解密 `.account`/`assh.yml`，无网络服务、无攻击者可控的重复计时 oracle；`--strict` 走公钥**验签**（非解密），不受影响。跟踪上游修复版 |
| RUSTSEC-2024-0449 | unsafe-libyaml 0.2.11 | warning（unmaintained） | 经 `serde_yaml` 引入 | 接受：仅用于本地 export/import YAML；后续可迁移到维护中的实现 |

## 备注

- 上游依赖 `github.com/keesely/kiris` **未进入本项目**（Rust 版按数据格式独立重写，无代码复制）。
- `serde_yaml` 上游已标记 deprecated；本项目仅用于 export/import 明文 YAML。
