//! SSH 连接 / 远程执行 / PTY 交互，基于 russh。
use anyhow::{Context, Result, bail};
use russh::client::{self, Handle};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key};
use russh::{ChannelMsg, Disconnect};
use std::sync::Arc;
use std::time::Duration;

pub struct Handler {
    pub strict: bool,
    pub host: String,
    pub port: u16,
}

impl client::Handler for Handler {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, k: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        // 默认行为完全不变（A9 冻结）：接受任意 host key。
        if !self.strict {
            return Ok(true);
        }
        let pk = match k {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(c) => c.public_key().clone().into(),
        };
        match verify_known_host(&self.host, self.port, &pk, &known_hosts_path()) {
            Ok(()) => Ok(true),
            Err(msg) => {
                eprintln!("Assh: {msg}");
                Ok(false)
            }
        }
    }
}

/// ~/.ssh/known_hosts 路径（用户 home 取自 getpwuid，与数据层一致）。
fn known_hosts_path() -> std::path::PathBuf {
    let home = crate::store::home_dir().unwrap_or_else(|| ".".into());
    std::path::Path::new(&home).join(".ssh").join("known_hosts")
}

/// 用 known_hosts 校验服务器公钥（`--strict` 使用）。
/// Ok(()) = 已知且匹配；否则返回可操作错误信息。
pub fn verify_known_host(
    host: &str,
    port: u16,
    pk: &russh::keys::ssh_key::PublicKey,
    path: &std::path::Path,
) -> Result<(), String> {
    match russh::keys::check_known_hosts_path(host, port, pk, path) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "未知主机 {host}:{port}（不在 {}）。\n  请先 `ssh -p {port} {host}` 连接一次，或执行\n  ssh-keyscan -p {port} {host} >> ~/.ssh/known_hosts",
            path.display()
        )),
        Err(russh::keys::Error::KeyChanged { line }) => Err(format!(
            "服务器指纹与 {} 第 {line} 行不一致，可能存在中间人攻击！\n  核对无误后再更新 known_hosts。",
            path.display()
        )),
        Err(e) => Err(format!("known_hosts 校验失败: {e}")),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Auth {
    pub user: String,
    pub password: Option<String>,
    /// (key_path, passphrase)
    pub key: Option<(String, Option<String>)>,
    /// `--strict`：用 ~/.ssh/known_hosts 校验 host key（默认 false，行为不变）
    pub strict: bool,
}

#[derive(Debug, Default)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// 按到达顺序合并 stdout+stderr（对应 Go 的 session.CombinedOutput）
    pub combined: Vec<u8>,
    pub exit_status: Option<u32>,
}

pub struct SshClient {
    session: Handle<Handler>,
    pub host: String,
    pub port: u16,
    pub user: String,
}

impl SshClient {
    pub async fn connect(host: &str, port: u16, auth: &Auth, timeout: Duration) -> Result<Self> {
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(20)),
            keepalive_max: 3,
            ..Default::default()
        });
        let addr = (host.to_string(), port);
        let mut session = tokio::time::timeout(
            timeout,
            client::connect(
                config,
                addr,
                Handler {
                    strict: auth.strict,
                    host: host.to_string(),
                    port,
                },
            ),
        )
        .await
        .context("连接超时")?
        .context("连接失败")?;

        let mut authed = false;
        if let Some(pw) = &auth.password {
            let r = session
                .authenticate_password(auth.user.clone(), pw.clone())
                .await?;
            authed = r.success();
        }
        if !authed && let Some((path, pass)) = &auth.key {
            let key = load_secret_key(path, pass.as_deref())
                .with_context(|| format!("读取私钥失败: {path}"))?;
            let hash = session.best_supported_rsa_hash().await?.flatten();
            let r = session
                .authenticate_publickey(
                    auth.user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await?;
            authed = r.success();
        }
        if !authed {
            bail!("认证失败");
        }
        Ok(SshClient {
            session,
            host: host.into(),
            port,
            user: auth.user.clone(),
        })
    }

    /// 执行单条命令，精确透传 stdout/stderr/exit status。
    pub async fn exec(&self, cmd: &str) -> Result<ExecResult> {
        let mut channel = self.session.channel_open_session().await?;
        channel.exec(true, cmd.as_bytes().to_vec()).await?;
        let mut out = ExecResult::default();
        loop {
            match channel.wait().await {
                None => break,
                Some(ChannelMsg::Data { data }) => {
                    out.stdout.extend_from_slice(&data);
                    out.combined.extend_from_slice(&data);
                }
                Some(ChannelMsg::ExtendedData { data, ext }) => {
                    if ext == 1 {
                        out.stderr.extend_from_slice(&data);
                        out.combined.extend_from_slice(&data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => out.exit_status = Some(exit_status),
                Some(ChannelMsg::Close) => break,
                _ => {}
            }
        }
        Ok(out)
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await?;
        Ok(())
    }

    pub fn handle(&self) -> &Handle<Handler> {
        &self.session
    }

    /// 交互式 shell（PTY）。返回远端退出码，或本地信号导致的 128+signo。
    /// TerminalGuard 负责进入/恢复 raw 模式；退出前先停 PTY writer。
    pub async fn shell(&self) -> Result<Option<u32>> {
        use crate::tty::{TerminalGuard, terminal_size};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::signal::unix::{SignalKind, signal};

        let channel = self.session.channel_open_session().await?;
        let (mut read, write) = channel.split();
        let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into());
        let ws = terminal_size(0);
        write
            .request_pty(true, &term, ws.cols, ws.rows, 0, 0, &[])
            .await?;
        write.request_shell(true).await?;

        let mut guard = TerminalGuard::enter_raw(0)?;
        let mut sigwinch = signal(SignalKind::window_change())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sighup = signal(SignalKind::hangup())?;
        let mut stdout = tokio::io::stdout();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
        tokio::spawn(async move {
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 4096];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut status: Option<u32> = None;
        let mut signalled: Option<u32> = None;
        loop {
            tokio::select! {
                data = rx.recv() => match data {
                    Some(d) => { write.data_bytes(d).await?; }
                    None => { write.eof().await.ok(); }
                },
                msg = read.wait() => match msg {
                    None | Some(ChannelMsg::Close) | Some(ChannelMsg::Eof) => break,
                    Some(ChannelMsg::Data { data }) => {
                        stdout.write_all(&data).await?;
                        stdout.flush().await?;
                    }
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                        stdout.write_all(&data).await?;
                        stdout.flush().await?;
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
                    Some(ChannelMsg::ExitSignal { .. }) => {}
                    Some(_) => {}
                },
                _ = sigwinch.recv() => {
                    let ws = terminal_size(0);
                    write.window_change(ws.cols, ws.rows, 0, 0).await.ok();
                }
                _ = sigterm.recv() => { signalled = Some(143); break; }
                _ = sigint.recv() => { signalled = Some(130); break; }
                _ = sighup.recv() => { signalled = Some(129); break; }
            }
        }
        // 先停 PTY writer，再恢复终端
        drop(rx);
        guard.restore();
        Ok(signalled.or(status))
    }
}

/// 建立 sftp 子系统会话。
pub async fn open_sftp(session: &Handle<Handler>) -> Result<russh_sftp::client::SftpSession> {
    let channel = session.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream()).await?;
    Ok(sftp)
}

#[cfg(test)]
mod strict_tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn key_path(which: &str, file: &str) -> Option<PathBuf> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tests")
            .join("sshd")
            .join(which)
            .join(file);
        p.exists().then_some(p)
    }

    fn read_pub(path: &std::path::Path) -> russh::keys::ssh_key::PublicKey {
        let content = std::fs::read_to_string(path).unwrap();
        russh::keys::ssh_key::PublicKey::from_openssh(content.trim()).unwrap()
    }

    /// 去掉 openssh 公钥串的注释，只留“类型 base64”。
    fn no_comment(pk: &russh::keys::ssh_key::PublicKey) -> String {
        let s = pk.to_openssh().unwrap();
        s.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
    }

    fn write_known_hosts(lines: &[String]) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "assh_known_hosts_test_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut f = std::fs::File::create(&p).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        p
    }

    /// 合法指纹：known_hosts 记录与出示公钥一致 → 通过。
    #[test]
    fn valid_fingerprint_passes() {
        let Some(pubpath) = key_path("hostkeys1", "ssh_host_ed25519_key.pub") else {
            eprintln!("跳过：测试 hostkey 不存在");
            return;
        };
        let pk = read_pub(&pubpath);
        // 线上收到的公钥无注释；PublicKey 的 PartialEq 含注释，需去注释后比
        let pk_wire = russh::keys::ssh_key::PublicKey::from_openssh(&no_comment(&pk)).unwrap();
        let kh = write_known_hosts(&[format!("testhost {}", no_comment(&pk))]);
        assert!(verify_known_host("testhost", 22, &pk_wire, &kh).is_ok());
        let _ = std::fs::remove_file(&kh);
    }

    /// 指纹不匹配：known_hosts 记录 A，服务器出示 B → 拒绝且提示中间人。
    #[test]
    fn tampered_key_is_rejected() {
        let (Some(p1), Some(p2)) = (
            key_path("hostkeys1", "ssh_host_ed25519_key.pub"),
            key_path("hostkeys2", "ssh_host_ed25519_key.pub"),
        ) else {
            eprintln!("跳过：测试 hostkey 不存在");
            return;
        };
        let recorded_a = read_pub(&p1);
        let server_b =
            russh::keys::ssh_key::PublicKey::from_openssh(&no_comment(&read_pub(&p2))).unwrap();
        let kh = write_known_hosts(&[format!("testhost {}", no_comment(&recorded_a))]);
        let err =
            verify_known_host("testhost", 22, &server_b, &kh).expect_err("应当因指纹不一致被拒绝");
        assert!(
            err.contains("不一致") || err.contains("中间人"),
            "err={err}"
        );
        let _ = (recorded_a, std::fs::remove_file(&kh));
    }

    /// 未知主机 → 拒绝且给出 ssh-keyscan/ssh 的可操作提示。
    #[test]
    fn unknown_host_gets_actionable_error() {
        let Some(pubpath) = key_path("hostkeys1", "ssh_host_ed25519_key.pub") else {
            eprintln!("跳过：测试 hostkey 不存在");
            return;
        };
        let pk = read_pub(&pubpath);
        let kh = write_known_hosts(&[]);
        let err = verify_known_host("unknown.example", 22, &pk, &kh).expect_err("未知主机应被拒绝");
        assert!(
            err.contains("ssh-keyscan") && err.contains("未知主机"),
            "err={err}"
        );
        let _ = std::fs::remove_file(&kh);
    }
}
