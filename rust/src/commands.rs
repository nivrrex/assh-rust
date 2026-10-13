//! 存储相关命令：ls / search / info / set / mv / rm / account。
use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::cli::{Flags, Invocation};
use crate::model::Server;
use crate::store::{Store, md5_hex};

type Data = BTreeMap<String, BTreeMap<String, Server>>;

fn strpad(str_: &str, pad: char, length: usize, pad_type: u8) -> String {
    let len = str_.chars().count();
    if len >= length {
        return str_.to_string();
    }
    let total = length - len;
    let (left, right) = match pad_type {
        2 => {
            let half = total / 2;
            (half, total - half)
        }
        1 => (0, total),
        _ => (total, 0),
    };
    format!(
        "{}{}{}",
        pad.to_string().repeat(left),
        str_,
        pad.to_string().repeat(right)
    )
}

fn srv_info(s: &Server) -> String {
    let flag = if !s.password.is_empty() {
        "passwd:yes"
    } else if !s.key.is_empty() {
        "pub key:yes"
    } else {
        "passwd:no"
    };
    format!("{}@{}:{} ({})", s.user, s.host, s.port, flag)
}

pub fn print_list(data: &Data) {
    let sep = "=".repeat(160);
    println!("{sep}");
    println!(
        "  {:<20} | {:<20} | {:<50} | {:<50} ",
        "Group Name", "Server Name", "Server Host", "Remark"
    );
    println!("{}", "-".repeat(160));
    let mut i = 0;
    for (g, ss) in data {
        for (n, s) in ss {
            i += 1;
            let color = if i % 4 == 0 { "\x1b[1m" } else { "\x1b[0m" };
            let info = srv_info(s);
            let remark = if s.remark.is_empty() {
                "\x1b[0;37m (no remark) \x1b[0m".to_string()
            } else {
                s.remark.clone()
            };
            println!(
                "> {color}{:<20}\x1b[0m | {color}{:<20}\x1b[0m | {color}{:<50}\x1b[0m | {} ",
                g, n, info, remark
            );
        }
    }
    println!("{sep}");
}

pub fn print_info(s: &Server) {
    println!("{}", "=".repeat(100));
    println!("{}", strpad(" Server Information Detail ", '+', 100, 2));
    println!("{}", "-".repeat(100));
    println!("{:>20}:   {}", "Server Name", s.name);
    println!("{:>20}:   {}", "Server Host", s.host);
    println!("{:>20}:   {}", "Server Port", s.port);
    println!("{:>20}:   {}", "Server User", s.user);
    println!("{:>20}:   {}", "Server Password", s.password);
    println!("{:>20}:   {}", "Server PemKey", s.key);
    println!("{:>20}:   {}", "Server Remark", s.remark);
    println!("{}", "=".repeat(100));
}

fn read_line(prompt: &str) -> String {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    buf.trim_end_matches(['\n', '\r']).to_string()
}

pub fn cmd_ls(store: &Store) {
    print_list(store.list());
}

pub fn cmd_search(store: &Store, q: Option<&String>) {
    let q = q.cloned().unwrap_or_default();
    print_list(&store.search(&q));
}

pub fn cmd_info(store: &Store, name: Option<&String>) {
    let Some(name) = name else { return };
    match store.get(name) {
        Some(s) => print_info(s),
        None => println!("Server [{name}] not found"),
    }
}

pub fn cmd_set(store: &mut Store, inv: &Invocation) -> Result<()> {
    let f: &Flags = &inv.flags;
    let name = inv.args.first().cloned().unwrap_or_default();
    let host = inv.args.get(1).cloned().unwrap_or_default();
    let mut server = Server::with_defaults();

    let exists = store.get(&name).is_some();
    if exists {
        server = store.get(&name).cloned().unwrap();
        if !f.force {
            let yes = read_line(&format!(
                "The server {name} is exists, do you sure cover it? [y/n]:"
            ));
            if yes.to_uppercase() != "Y" {
                return Ok(());
            }
        }
    }
    if !host.is_empty() {
        let (h, u, p) = Server::parse_host_string(&host);
        if !h.is_empty() {
            server.host = h;
            if p > 0 {
                server.port = p;
            }
            if !u.is_empty() {
                server.user = u;
            }
        }
    }
    if let Some(h) = &f.host {
        server.host = h.clone();
    }
    if server.host.is_empty() {
        bail!("set server: hostname is nil");
    }
    if let Some(u) = &f.login {
        server.user = u.clone();
    }
    if let Some(p) = f.port
        && p > 0
    {
        server.port = p;
    }
    if let Some(p) = &f.password {
        server.password = p.clone();
    }
    if let Some(k) = &f.key {
        server.key = k.clone();
    }
    if let Some(r) = &f.remark {
        server.remark = r.clone();
    }
    server.name = name.clone();
    store.set_server(&name, server.clone());
    store.save()?;
    println!("Server [{name}] success.");
    print_info(store.get(&name).unwrap_or(&server));
    Ok(())
}

fn del_one(store: &mut Store, name: &str, force: bool) -> Result<()> {
    if store.get(name).is_some() {
        let yes = if force {
            "Y".to_string()
        } else {
            read_line(&format!("Are you sure (delete {name}) [y/n]: "))
        };
        if yes.to_uppercase() == "Y" {
            store.del(name)?;
            store.save()?;
        }
    } else {
        println!("服务器({name})不存在于服务器列表中.");
    }
    Ok(())
}

pub fn cmd_rm(store: &mut Store, inv: &Invocation) -> Result<()> {
    let f = &inv.flags;
    let name = inv.args.first().cloned().unwrap_or_default();
    if f.group {
        if name.is_empty() {
            println!("group name is nil");
            return Ok(());
        }
        let members: Vec<String> = store
            .get_group(&name)
            .map(|g| g.values().map(|s| s.name.clone()).collect())
            .unwrap_or_default();
        if members.is_empty() {
            println!("group no exists.");
            return Ok(());
        }
        // 合成原版行为：delServer 传入的是短名，Get 后在空组下找不到 → 均“不存在”
        for m in members {
            del_one(store, &m, f.force)?;
        }
        return Ok(());
    }
    if !name.is_empty() {
        del_one(store, &name, f.force)?;
    }
    Ok(())
}

pub fn cmd_mv(store: &mut Store, inv: &Invocation) -> Result<()> {
    let from = inv.args.first().cloned().unwrap_or_default();
    let to = inv.args.get(1).cloned().unwrap_or_default();
    store.move_server(&from, &to)?;
    store.save()?;
    Ok(())
}

pub fn cmd_account(store: &mut Store, inv: &Invocation) -> Result<()> {
    if store.basedir.join(".account").exists() {
        let c_passwd = read_line("Current Password: ");
        if md5_hex(c_passwd.as_bytes()) != store.passwd_hex() {
            println!("[ERROR] passwd: Authentication failed");
            return Ok(());
        }
    }
    let n_passwd = inv
        .args
        .first()
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| read_line("New Password: "));
    let a_passwd = read_line("Retrype Passwod: ");
    if md5_hex(n_passwd.as_bytes()) != md5_hex(a_passwd.as_bytes()) {
        println!("[ERROR] passwd: Inconsistent, please try again.");
        return Ok(());
    }
    store.change_master(&n_passwd)?;
    println!("success.");
    Ok(())
}

// ---------------- 连接目标解析（含存储） ----------------

pub fn resolve_server(store: Option<&Store>, inv: &Invocation) -> Result<Server> {
    let f = &inv.flags;
    let name_arg = match inv.command {
        Some(crate::cli::Command::Login)
        | Some(crate::cli::Command::Push)
        | Some(crate::cli::Command::Pull)
        | Some(crate::cli::Command::Bc)
        | Some(crate::cli::Command::Ping) => inv.args.first().cloned(),
        _ => inv.server.clone(),
    };
    let mut s = if let (Some(st), Some(n)) = (store, name_arg.as_ref()) {
        st.get(n).cloned().unwrap_or_else(Server::with_defaults)
    } else {
        Server::with_defaults()
    };
    if let Some(name) = &name_arg
        && s.host.is_empty()
    {
        let (h, u, p) = Server::parse_host_string(name);
        if !h.is_empty() {
            s.host = h;
            if !u.is_empty() {
                s.user = u;
            }
            if p > 0 {
                s.port = p;
            }
        }
    }
    if let Some(h) = &f.host {
        s.host = h.clone();
    }
    if s.host.is_empty() {
        bail!("login server: hostname is nil");
    }
    if let Some(u) = &f.login {
        s.user = u.clone();
    }
    if let Some(p) = f.port
        && p > 0
    {
        s.port = p;
    }
    if let Some(p) = &f.password {
        s.password = p.clone();
    }
    if let Some(k) = &f.key {
        s.key = k.clone();
    }
    if s.name.is_empty() {
        s.name = format!("{}@{}:{}", s.user, s.host, s.port);
    }
    Ok(s)
}

pub fn auth_of(s: &Server, strict: bool) -> crate::ssh::Auth {
    let mut a = crate::ssh::Auth {
        user: s.user.clone(),
        strict,
        ..Default::default()
    };
    if !s.password.is_empty() {
        a.password = Some(s.password.clone());
    }
    if !s.key.is_empty() {
        let path = crate::store::expand_tilde(&s.key);
        let pass = if s.password.is_empty() {
            None
        } else {
            Some(s.password.clone())
        };
        a.key = Some((path, pass));
    }
    a
}

// ---------------- push / pull ----------------

fn remote_join(dir: &str, base: &str) -> String {
    let d = crate::sftp::remote_real_path(dir);
    let d = if d.is_empty() || d == "." || d == "./" {
        String::new()
    } else {
        d.trim_end_matches('/').to_string()
    };
    if d.is_empty() {
        base.to_string()
    } else {
        format!("{d}/{}", base.trim_start_matches('/'))
    }
}

fn base_name(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

pub async fn cmd_push(
    s: &Server,
    local: &[String],
    remote: &str,
    buf: usize,
    strict: bool,
) -> Result<()> {
    use crate::sftp::{DEFAULT_BUF, push_path};
    use crate::ssh::SshClient;
    use crate::ssh::open_sftp;
    use std::time::Instant;

    let _ = DEFAULT_BUF;
    let start = Instant::now();
    let cli = SshClient::connect(
        &s.host,
        s.port,
        &auth_of(s, strict),
        std::time::Duration::from_secs(30),
    )
    .await?;
    let sftp = open_sftp(cli.handle()).await?;
    let mut first_err: Option<anyhow::Error> = None;
    for (n, l) in local.iter().enumerate() {
        let lp = crate::sftp::local_real_path(l);
        let rf = remote_join(remote, &base_name(l));
        println!("{}: '{}' -> {}@{}:{}", n, lp.display(), s.user, s.host, rf);
        let r = push_path(&sftp, l, remote, buf).await;
        if let Err(e) = r
            && first_err.is_none()
        {
            first_err = Some(e);
        }
    }
    cli.disconnect().await.ok();
    println!("> SFTP PUSH time cost: {:?}", start.elapsed());
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub async fn cmd_pull(s: &Server, remote: &[String], local: &str, strict: bool) -> Result<()> {
    use crate::sftp::pull_path;
    use crate::ssh::SshClient;
    use crate::ssh::open_sftp;

    let cli = SshClient::connect(
        &s.host,
        s.port,
        &auth_of(s, strict),
        std::time::Duration::from_secs(30),
    )
    .await?;
    let sftp = open_sftp(cli.handle()).await?;
    for (n, r) in remote.iter().enumerate() {
        let rr = crate::sftp::remote_real_path(r);
        // 对齐 oracle：先 Stat，失败则不打印该行
        if sftp.metadata(rr.clone()).await.is_err() {
            anyhow::bail!("remote file not found! No such file: No such file");
        }
        let local_file = format!("{}/{}", local.trim_end_matches('/'), base_name(&rr));
        println!("{}: '{}@{}:{}' -> {}", n, s.user, s.host, rr, local_file);
        pull_path(&sftp, r, local).await?;
    }
    cli.disconnect().await.ok();
    Ok(())
}

// ---------------- ping ----------------

pub async fn cmd_ping(store: &Store, strict: bool) {
    use crate::ssh::SshClient;
    let sep = "=".repeat(160);
    println!("{sep}");
    println!(
        " {:<20} | {:<50} | {:<50} ",
        "Server Name", "Server Host", "Result"
    );
    println!("{}", "-".repeat(160));
    let mut rows = vec![];
    for ss in store.list().values() {
        for s in ss.values() {
            let info = srv_info(s);
            let start = std::time::Instant::now();
            let res = SshClient::connect(
                &s.host,
                s.port,
                &auth_of(s, strict),
                std::time::Duration::from_secs(30),
            )
            .await;
            let (color, remark) = match res {
                Ok(c) => {
                    c.disconnect().await.ok();
                    ("\x1b[0m", format!("time cost: {:?}", start.elapsed()))
                }
                Err(e) => ("\x1b[31m", e.to_string()),
            };
            rows.push(format!(
                "> {color}{:<20}\x1b[0m | {color}{:<50}\x1b[0m | {color}{}\x1b[0m",
                s.name, info, remark
            ));
        }
    }
    for r in rows {
        eprintln!("{r}");
    }
    println!("{sep}");
}

// ---------------- export / import ----------------

#[derive(serde::Serialize, serde::Deserialize)]
struct ExportServer {
    name: String,
    host: String,
    port: u16,
    user: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    remark: String,
    #[serde(default)]
    pemkey: std::collections::BTreeMap<String, String>,
}

pub fn cmd_export(store: &Store, path: &str) -> Result<()> {
    use std::collections::BTreeMap;
    let mut top: BTreeMap<String, BTreeMap<String, ExportServer>> = BTreeMap::new();
    for (g, ss) in store.list() {
        let gk = if g.is_empty() {
            "nil".to_string()
        } else {
            g.clone()
        };
        let gm = top.entry(gk).or_default();
        for (n, s) in ss {
            let pemkey = if s.key.is_empty() {
                std::collections::BTreeMap::new()
            } else {
                let p = crate::store::expand_tilde(&s.key);
                let private = std::fs::read_to_string(&p).unwrap_or_default();
                let public = std::fs::read_to_string(format!("{p}.pub")).unwrap_or_default();
                std::collections::BTreeMap::from([
                    ("private".to_string(), private),
                    ("public".to_string(), public),
                ])
            };
            gm.insert(
                n.clone(),
                ExportServer {
                    name: s.name.clone(),
                    host: s.host.clone(),
                    port: s.port,
                    user: s.user.clone(),
                    password: s.password.clone(),
                    remark: s.remark.clone(),
                    pemkey,
                },
            );
        }
    }
    let out = serde_yaml::to_string(&top)?;
    std::fs::write(path, out)?;
    Ok(())
}

pub fn cmd_import(store: &mut Store, path: &str, force: bool) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    let top: BTreeMap<String, BTreeMap<String, ExportServer>> = serde_yaml::from_str(&text)?;
    for (g, servers) in top {
        for (n, es) in servers {
            let name = if g == "nil" {
                n.clone()
            } else {
                format!("{g}.{n}")
            };
            println!("import <  {name}");
            if !force && store.get(&name).is_some() {
                let yes = read_line(&format!(
                    "The server {name} is exists, do you sure cover it? [y/n]:"
                ));
                if yes.to_uppercase() != "Y" {
                    continue;
                }
            }
            let mut s = Server {
                name: if es.name.is_empty() {
                    name.clone()
                } else {
                    es.name.clone()
                },
                host: es.host.clone(),
                port: es.port,
                user: es.user.clone(),
                password: es.password.clone(),
                key: String::new(),
                remark: es.remark.clone(),
                options: None,
            };
            if let Some(priv_content) = es.pemkey.get("private")
                && !priv_content.is_empty()
            {
                let tmp = format!("/tmp/_assh_pemkey_{name}");
                std::fs::write(&tmp, priv_content)?;
                s.key = tmp.clone();
                if let Some(pub_content) = es.pemkey.get("public")
                    && !pub_content.is_empty()
                {
                    std::fs::write(format!("{tmp}.pub"), pub_content)?;
                }
            }
            store.set_server(&name, s);
            println!("imported:  {name}");
        }
    }
    store.save()?;
    Ok(())
}

/// 单服务器执行一条命令（用于 bc 批量），输出格式对齐 _connection。
pub async fn exec_once(s: &Server, cmd: &str, strict: bool) -> anyhow::Result<()> {
    use crate::ssh::SshClient;
    let hostport = format!("{}@{}:{}", s.user, s.host, s.port);
    let passwd_flag = if !s.key.is_empty() {
        "(pemKey: yes)".to_string()
    } else if !s.password.is_empty() {
        "(password: yes)".to_string()
    } else {
        "(password: no)".to_string()
    };
    println!("> connection: {} {} ", hostport, passwd_flag);
    let start = std::time::Instant::now();
    let cli = SshClient::connect(
        &s.host,
        s.port,
        &auth_of(s, strict),
        std::time::Duration::from_secs(30),
    )
    .await?;
    let res = cli.exec(cmd).await?;
    println!(
        "\n> Result ({} -> '{}'): \n{}",
        s.name,
        hostport,
        String::from_utf8_lossy(&res.combined)
    );
    cli.disconnect().await.ok();
    println!(">  time cost: {:?}", start.elapsed());
    Ok(())
}
