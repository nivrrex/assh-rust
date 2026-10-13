# Changelog

## 未发布 — 首次运行初始化修复（2026-10-13）

- [接口变更] 新增 `Store::ensure_layout()`；`Store::open_default()` 先初始化再打开（幂等）【已验证事实】
- [接口变更] `Store::open()` 容忍 `.account` 缺失（passwd 为空）；`servers.db` 存在但 `.account` 缺失时报错提示先 `assh account`【已验证事实】
- [接口变更] `Store::save()` 在未设口令时返回 Err（原先会 panic）；`change_master()` 首次设密不写 `servers.db`【已验证事实】
- [决策] 在 `open_default()` 内初始化，否决新增 `assh init` 子命令，原因：上游无此命令，会破坏兼容面【已验证事实】
- [决策] `.account` 缺失时以空口令打开，使 `account` 能首次设密；否决单独为 account 写初始化分支（会重复上游 init 逻辑）【已验证事实】
- [决策] `-v`/`--help` 不触发初始化（有意差异 BD-116），否决复刻上游“包 init() 连 -v 都建目录”（只读查询不应有副作用）【已验证事实】
- [决策] 新生成 `.rsa` 权限 `0600`，否决复刻上游 `0644`（私钥不应 world-readable）【已验证事实】
- [失败尝试] `ssh-keygen -t ed25519 -b ""` 被 OpenSSH 9.2 拒绝（`Bits has bad value`）；`tools/make_keys.sh` 改为仅在指定位数时传 `-b`【已验证事实】
- [未解决] `VERSION` 仍为 `v1.0.4-compat`，与修复前构建无法从版本串区分（待用户决定是否 bump）【已验证事实】

## v1.0.4-compat (2026-10-13)

首个公开版本：keesely/assh v1.0.4 的 Rust 重写。

- 兼容上游 v1.0.3 / v1.0.4 的 `~/.assh` 数据（读 + 原子写，双向可读）。
- 命令：`login` / `-c`、`ls`、`search`、`info`、`set`、`mv`、`rm`、`account`、
  `push`、`pull`、`export`、`import`、`bc`、`ping`、`version`、`help`。
- 认证：口令、RSA、Ed25519（含 passphrase）。
- 修复上游缺陷：写库不截断、错误静默吞掉、`-c` 退出码、认证/`mv` panic、终端 raw 残留。
- 新增可选 `--strict`（`~/.ssh/known_hosts` 校验）。
- 未纳入：`keygen`、`proxy`、`hostproxy`、`sync`、`upgrade`。

详见 [与上游差异](docs/COMPATIBILITY.md) 与 [数据格式](docs/DATA-FORMAT.md)。
