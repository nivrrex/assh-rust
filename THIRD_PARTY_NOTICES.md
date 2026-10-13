# THIRD_PARTY_NOTICES

## 本项目（assh-rs）

- 许可：MIT，Copyright (c) 2026 assh-rs contributors（全文见根目录 `LICENSE`）。

## keesely/assh（行为与格式基准，上游）

- 项目：https://github.com/keesely/assh
- 版本：v1.0.4（tag `068482d`）
- 许可：MIT，Copyright (c) 2019 kee

本项目（assh-rs）是独立重写：**未复制上游 Go 源码**，仅以其可观察行为与数据格式为兼容目标
（对照二进制见 `oracles/`，经 `tools/fetch_oracles.sh` 获取，不入库）。上游许可全文如下：

```
MIT License

Copyright (c) 2019 kee

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## github.com/keesely/kiris

- 用途（仅上游）：assh v1 的加密/文件/YAML 工具依赖。
- 许可：**上游未声明 LICENSE**。
- 本项目：**kiris 代码未进入本项目**——Rust 版按数据格式独立重写；Python 参考实现亦为独立编写。
  仅用于阅读理解格式（详见开发记录（非公开文档））。

## Rust 依赖

`russh`、`russh-sftp`、`tokio`、`serde`、`rsa`、`aes`、`cbc`、`md-5`、`nix`、`libc` 等，
均为 MIT / Apache-2.0 宽松许可（无 GPL）。版本、许可与安全审计见 `docs/DEPENDENCIES.md`。

## Python / 测试工具

`cryptography`（Apache-2.0/BSD）、`pexpect`（ISC），仅用于 `tools/` 与测试。
