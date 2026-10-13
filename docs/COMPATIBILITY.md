# 与上游 assh v1.0.4 的差异

assh-rs 以 **keesely/assh v1.0.4** 为兼容基线：读取其数据、对齐其命令行为。
下面是**有意的行为差异**，均为缺陷修复或可用性/安全改进；其余行为保持一致
（conformance 差分 64 用例对齐，见 `tests/`）。

## 缺陷修复类

| 项 | 上游 v1.0.4 | assh-rs | 说明 |
|---|---|---|---|
| `-c` 退出码 | 恒为 0 | 回传远端退出码（`exit 7` → 7） | 脚本可用性 |
| 认证失败（错误口令/用户） | panic（nil AuthMethod） | 报错并以非零退出 | 健壮性 |
| `mv` 源不存在 | panic（nil 解引用） | 报错并退出 | 健壮性 |
| 错误处理 | `logLevel=OFF` 时 `log.Fatal` 静默吞错 | 非零退出 + stderr | 可诊断性 |
| 写库 | `O_WRONLY` 不截断 → 收缩写损坏库 | 临时文件 + rename 原子写 | 数据安全 |
| `-P` 口令回显 | login 时明文回显 | 不回显 | 安全 |
| keepalive | `Options` 恒 nil → 实际关闭 | 默认 20s | 连接保活 |
| 认证 panic 后的终端 | 可能残留 raw | 统一恢复（见下） | 可用性 |
| 首次运行初始化 | `~/.assh` 不存在时各命令报 `读取失败: .rsa` 且 rc=1 | 自动初始化 `~/.assh`（对齐上游 `init()`） | 可用性 |
| `.rsa`/`.rsa.pub` 为目录 | 直接失败 | 明确报错并提示删除后重试 | 可诊断性 |
| `servers.db` 在但 `.account` 缺 | （不一致） | 报错提示先执行 `assh account` | 可诊断性 |

## 终端（TTY）行为

- 上游在 SIGTERM / SIGINT / SIGHUP 下**不恢复**终端（遗留 raw，出现 `;xxx` 乱码）。
- assh-rs 进入 shell 前保存 termios，正常退出与上述信号、网络断开后均**幂等恢复**；
  交互期间 Ctrl+C 直接转发远端（与上游一致）。
- `SIGKILL`（`kill -9`）为 OS 级不可恢复项；如遇残留执行 `stty sane` 或 `reset`。
- `-c` 非交互模式不改变终端模式。

## 可用性 / 格式

| 项 | 上游 | assh-rs |
|---|---|---|
| 全局 flag 位置 | `assh -H y login x` 丢弃 flag | 等价于 `login x -H y` |
| `-v` / `--help` 副作用 | 包 `init()` 即建 `~/.assh` | **不触发**初始化（无文件副作用） |
| 帮助 / 版本 | 列全部命令（含未实现）；`v1.0.4-20230322` | **只列已实现命令 + `--strict`**；`version` 输出 **`v1.0.4-compat`** |
| ping 进度行 | 打印 `Scanning: n/n` | 不复刻（结果一致） |
| export 分组顺序 | Go map 随机 | 稳定排序 |

## 安全加固（可选）

- 上游 `HostKeyCallback` 恒接受任意 host key。
- assh-rs **默认行为不变**；新增可选 `--strict`：用 `~/.ssh/known_hosts` 校验，
  未知主机/指纹不匹配给出可操作错误。TOFU 暂未实现。

## 未纳入的命令

`keygen`、`proxy`、`hostproxy`、`sync`、`upgrade` 暂未实现（裁剪范围）。

## 上游后续版本

keesely/assh 在 v1.0.4 之后为 **v2 完整重写**（改用 SQLite + AES-GCM、移除 `kiris`、
命令重排为 `proxy`/`tunnel`），其数据格式与 v1 **不兼容**。本项目的兼容目标是 v1 数据，
因此不对照 v2。
