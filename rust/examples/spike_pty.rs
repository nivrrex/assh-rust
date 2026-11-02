//! spike: 交互 PTY + SIGWINCH 窗口变化转发。
//! 用法: spike_pty <host> <port> <user> <auth>
use std::time::Duration;

use assh::ssh::{Auth, SshClient};
use assh::tty::{TerminalGuard, terminal_size};
use russh::ChannelMsg;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{SignalKind, signal};

fn parse_auth(s: &str, user: &str) -> anyhow::Result<Auth> {
    let mut a = Auth {
        user: user.to_string(),
        ..Default::default()
    };
    if let Some(pw) = s.strip_prefix("pw:") {
        a.password = Some(pw.to_string());
    } else if let Some(rest) = s.strip_prefix("keypass:") {
        let (path, pass) = rest.split_once(':').expect("keypass:<path>:<pass>");
        a.key = Some((path.to_string(), Some(pass.to_string())));
    } else if let Some(path) = s.strip_prefix("key:") {
        a.key = Some((path.to_string(), None));
    } else {
        anyhow::bail!("未知 auth 形式: {s}");
    }
    Ok(a)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("用法: spike_pty <host> <port> <user> <auth>");
        std::process::exit(2);
    }
    let (host, port, user, auth) = (&args[0], args[1].parse::<u16>()?, &args[2], &args[3]);
    let auth = parse_auth(auth, user)?;
    let cli = SshClient::connect(host, port, &auth, Duration::from_secs(15)).await?;

    let channel = cli.handle().channel_open_session().await?;
    let (mut read, write) = channel.split();
    let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into());
    let ws = terminal_size(0);
    write
        .request_pty(true, &term, ws.cols, ws.rows, 0, 0, &[])
        .await?;
    write.request_shell(true).await?;

    let guard = TerminalGuard::enter_raw(0)?;
    let mut sigwinch = signal(SignalKind::window_change())?;
    let mut stdout = tokio::io::stdout();
    let mut status: Option<u32> = None;

    // 独立任务读 stdin，避免 select 分支间对同一 buffer 的借用冲突
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

    loop {
        tokio::select! {
            data = rx.recv() => match data {
                Some(d) => { write.data_bytes(d).await?; }
                None => { write.eof().await.ok(); }
            },
            msg = read.wait() => match msg {
                None | Some(ChannelMsg::Close) | Some(ChannelMsg::Eof) => break,
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    status = Some(exit_status);
                }
                Some(ChannelMsg::Data { data }) => {
                    stdout.write_all(&data).await?;
                    stdout.flush().await?;
                }
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                    stdout.write_all(&data).await?;
                    stdout.flush().await?;
                }
                Some(other) => eprintln!("[spike_pty][msg] {other:?}"),
            },
            _ = sigwinch.recv() => {
                let ws = terminal_size(0);
                write.window_change(ws.cols, ws.rows, 0, 0).await.ok();
            }
        }
    }
    drop(guard);
    cli.disconnect().await.ok();
    // 避免 tokio runtime 在退出时等待 stdin 阻塞线程；终端已恢复，直接退出。
    std::process::exit(status.unwrap_or(0) as i32);
}
