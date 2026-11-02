# assh-rs

[keesely/assh](https://github.com/keesely/assh) v1.0.4 的 **Rust 重写**（裁剪 + 兼容）。

- **兼容存量数据**：可直接读写上游 v1.0.3 / v1.0.4 生成的 `~/.assh`（见 [数据格式](docs/DATA-FORMAT.md)）。
- **修复上游缺陷**：写库损坏、错误静默、`-c` 退出码、终端 raw 残留等（见 [与上游差异](docs/COMPATIBILITY.md)）。
- 基于 [`russh`](https://crates.io/crates/russh) 的纯 Rust 实现，无 CGO 依赖。

## 来源与许可

- 基于上游 **keesely/assh v1.0.4** 的**行为与数据格式独立重写**，未复制其源码。
- 本项目：**MIT**（见 [`LICENSE`](LICENSE)）；上游亦为 **MIT, Copyright (c) 2019 kee**，
  第三方声明见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。

## 功能

| 能力 | 状态 |
|---|---|
| 直连 / `login`、`-c` 远程命令（精确 stdout/stderr/退出码） | ✅ |
| `ls` / `search` / `info` | ✅ |
| `set` / `mv` / `rm`（含 `-f`、`-g`、`-R`、`-p`） | ✅ |
| `account`（首次设置 / 改密，全库重加密） | ✅ |
| `push` / `pull`（SFTP，`-b` 分块） | ✅ |
| `export` / `import`（明文 YAML） | ✅ |
| `bc`（分组批量）、`ping` | ✅ |
| 密码 / RSA / Ed25519 私钥（含 passphrase）认证 | ✅ |
| PTY 交互、窗口变化转发、终端恢复 | ✅ |
| 可选 `--strict`（`~/.ssh/known_hosts` 校验） | ✅ |
| `keygen` / `proxy` / `sync` / `upgrade` | ⛔ 未纳入 |

## 安装 / 构建

- **预编译二进制**：见 [Releases](https://github.com/nivrrex/assh-rust/releases)
  （Linux x86-64；校验见附带的 `SHA256SUMS`）。
- **自行构建**（需要 Rust 工具链）：

```bash
cd rust && cargo build --release      # 产物 rust/target/release/assh
./check.sh                            # fmt + clippy -D warnings + 测试
```

## 使用

```bash
# 首次运行：自动创建 ~/.assh 并设置主口令（也可先 account 再 set）
assh account 'your-master-password'

# 添加服务器
assh set myserver -H 1.2.3.4 -p 22 -l root -P 'password' -R '备注'

# 列表 / 详情 / 搜索
assh ls
assh info myserver
assh search web

# 交互登录
assh myserver            # 或 assh login myserver

# 执行远程命令（退出码透传）
assh myserver -c 'uname -a'
assh myserver -c 'exit 7'; echo $?        # → 7

# 传输文件（远端参数为目录）
assh push myserver ./local.txt /tmp/
assh pull myserver /tmp/remote.txt ./

# 严格校验 host key
assh --strict myserver -c 'id'
```

全局 flag（`-H -p -l -P -k -c --log --llv --strict`）可置于命令前后任意位置。

## 终端行为

- 交互 shell 退出（正常 / `logout` / SIGTERM / SIGINT / SIGHUP / 断网）后，终端设置**自动恢复**。
- `SIGKILL`（`kill -9`）无法捕获；若终端残留 raw，执行 `stty sane` 或 `reset`。
- Ctrl+C 在交互期间**转发远端**（与上游一致）；`-c` 模式不改变终端模式。

## 安全

- 默认与上游一致，**接受任意 host key**（存在中间人风险，建议仅用于可信网络）。
- 启用 `--strict` 后用 `~/.ssh/known_hosts` 校验服务器公钥；未知主机/指纹不匹配会拒绝并给出
  可操作提示（`ssh` 连接一次或 `ssh-keyscan`）。
- 依赖安全审计见 [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md)。

## 文档

- [数据格式 `~/.assh`](docs/DATA-FORMAT.md)
- [与上游 assh v1.0.4 的差异](docs/COMPATIBILITY.md)
- [依赖与安全审计](docs/DEPENDENCIES.md)
- [变更记录](CHANGELOG.md)

## 测试

```bash
python3 tools/make_fixtures.py     # 生成测试 fixtures
make conformance                   # oracle 基线自洽
python3 tests/data_layers.py       # 数据层 L1/L2/L3 + account
```

测试使用专用用户与本地 sshd 容器；所有写操作仅在 fixture 副本上。

## 许可

MIT，见 [`LICENSE`](LICENSE)。
