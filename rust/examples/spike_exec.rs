//! spike: 密码 / 公钥（含 passphrase）认证 + exec 精确取回 stdout/stderr/退出码。
//! 用法: spike_exec <host> <port> <user> <auth> <cmd...>
//!   <auth> = pw:<password> | key:<path> | keypass:<path>:<passphrase>

use assh::ssh::{Auth, SshClient};

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
    if args.len() < 5 {
        eprintln!("用法: spike_exec <host> <port> <user> <auth> <cmd...>");
        std::process::exit(2);
    }
    let host = &args[0];
    let port: u16 = args[1].parse()?;
    let user = &args[2];
    let auth = parse_auth(&args[3], user)?;
    let cmd = args[4..].join(" ");

    let cli = SshClient::connect(
        host,
        port,
        &auth,
        std::time::Duration::from_secs(
            std::env::var("SPIKE_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
        ),
    )
    .await?;
    let res = cli.exec(&cmd).await?;
    print!("{}", String::from_utf8_lossy(&res.stdout));
    eprint!("{}", String::from_utf8_lossy(&res.stderr));
    eprintln!("[spike_exec] exit_status={:?}", res.exit_status);
    cli.disconnect().await.ok();
    std::process::exit(res.exit_status.unwrap_or(0) as i32);
}
