# Changelog

## 未发布 — 重复 set / mv 私钥丢失修复（2026-11-02）

- [缺陷修复] 对已存在服务器再次 `set` 时，`copy_pem_key` 把 `data/<name>/<key>` 自我复制；
  `std::fs::copy` 先截断目标，导致私钥与其 `.pub` 均变为 0 字节，随后登录报“读取私钥失败”。
  改为按 `dev+inode` 判定源/目标是否同一文件，是则跳过复制（仍返回原路径）【已验证事实】
- [缺陷修复] `Store::set_server` 先写入目标 `name` 再复制私钥。此前 `mv` 复用该函数时按旧名算目录，
  复制落到旧目录并被随后的 `del(from)` 删除，私钥丢失；现随服务器移到 `data/<新名>/`【已验证事实】
- [测试] 新增单测 `store::tests::reset_and_move_keep_pem_key`（重复 set 内容不变、mv 后路径与内容正确）【已验证事实】
- [验证] `cargo test` 27/27；`tests/data_layers.py` ALL PASS；真实库执行用户原命令 `set pi` 后
  `assh pi -c 'echo SMOKE_OK'` rc=0【已验证事实】
- [构建] `cli.rs` 参数解析尾部嵌套 `else { if }` 合并为 `else if`（Rust 1.90 clippy 新增
  `collapsible_else_if`，语义不变）；`bash rust/check.sh` 现 fmt/clippy/test/examples 全部通过【已验证事实】
- [未解决] `tests/data_layers.py` 在 `fixtures/fixture-real/` 存在但缺 `.assh` 时抛 `FileNotFoundError`，
  本机验证时临时移开该目录绕过；脚本缺省判断应检查 `.assh` 是否存在【已验证事实】

## 2026-10-13 开发包快照（dev，含首次运行初始化修复）

- [接口变更] 新增 `Store::ensure_layout()`；`open_default()` 先初始化 `~/.assh` 再打开（幂等）【已验证事实】
- [接口变更] `open()` 容忍 `.account` 缺失；`save()` 未设口令返回 Err；首次 `account` 不写 `servers.db`【已验证事实】
- [决策] 初始化放在 `open_default()`，否决新增 `assh init` 子命令（上游无此命令）【已验证事实】
- [决策] `-v`/`--help` 不触发初始化（BD-116），否决复刻上游包 `init()` 的副作用【已验证事实】
- [失败尝试] `ssh-keygen -t ed25519 -b ""` 被 OpenSSH 9.2 拒绝；`tools/make_keys.sh` 改为仅指定位数时才传 `-b`【已验证事实】
- [假设更新] 测试驱动此前依赖调用者 stdin（继承 tty 时 `push <未知服务器>` 阻塞）；已固定 `stdin=DEVNULL`【已验证事实】
- [未解决] `keygen`/`proxy`/`hostproxy`/`sync`/`upgrade` 未实现；`ping` 进度行未复刻【已验证事实】
- [未解决] `cargo audit`：RUSTSEC-2023-0071（rsa Marvin，无修复版，接受）、RUSTSEC-2024-0449（unsafe-libyaml，接受）【已验证事实】
- [未解决] 测试 golden/脚本内嵌测试用户 home（`/home/asshtest`）；跨机器按 `tests/harness/env.sh` 调整【已验证事实】

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
