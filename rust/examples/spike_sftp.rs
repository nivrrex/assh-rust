//! spike: SFTP 上传/下载（含 >1MiB 文件分块校验）。
//! 用法: spike_sftp <host> <port> <user> <auth> push|pull <local> <remote> [buf]
use std::time::Duration;

use assh::sftp::{ensure_buf, pull_path, push_path};
use assh::ssh::{Auth, SshClient, open_sftp};

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
    if args.len() < 7 {
        eprintln!("用法: spike_sftp <host> <port> <user> <auth> push|pull <local> <remote> [buf]");
        std::process::exit(2);
    }
    let (host, port, user, auth, op, local, remote) = (
        &args[0],
        args[1].parse::<u16>()?,
        &args[2],
        &args[3],
        &args[4],
        &args[5],
        &args[6],
    );
    let buf = ensure_buf(args.get(7).and_then(|s| s.parse::<i64>().ok()));
    let auth = parse_auth(auth, user)?;

    let cli = SshClient::connect(host, port, &auth, Duration::from_secs(15)).await?;
    let sftp = open_sftp(cli.handle()).await?;
    match op.as_str() {
        "push" => push_path(&sftp, local, remote, buf).await?,
        "pull" => pull_path(&sftp, remote, local).await?,
        other => anyhow::bail!("未知操作: {other}"),
    }
    println!("[spike_sftp] {op} ok (buf={buf})");
    cli.disconnect().await.ok();
    Ok(())
}
