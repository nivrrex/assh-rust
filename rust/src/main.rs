use std::time::{Duration, Instant};

use assh::cli::{self, Command, Invocation};
use assh::commands as cmds;
use assh::model::Server;
use assh::ssh::{Auth, SshClient};
use assh::store::Store;

pub const VERSION: &str = "v1.0.4-compat";

fn prog_name() -> String {
    std::env::args()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "assh".to_string())
}

const COMMAND_LIST: &[(&str, &str)] = &[
    ("version", ""),
    ("account", "Set the data crypto key (password)"),
    ("ls", "Print all servers list"),
    ("search", "Search the server name"),
    ("info", "Print the server information"),
    ("set", "Add/Modify the server"),
    ("mv", "Move/Rename Server Name"),
    ("rm", "Remove Server"),
    ("login", "login server or exec remote server command"),
    ("bc", "batch remote command execution"),
    ("push", "sftp push file to remote server"),
    ("pull", "sftp pull file from remote server"),
    ("export", "export servers to YAML (plaintext)"),
    ("import", "import servers from YAML"),
    ("ping", "ping all servers"),
    (
        "help, h",
        "Shows a list of commands or help for one command",
    ),
];

const GLOBAL_OPTIONS: &[(&str, &str)] = &[
    ("-H value", "server host"),
    ("-p value", "bind address port (default: 0)"),
    ("-l value", "login name"),
    ("-P value", "login password"),
    ("-k value", "automatic login public key"),
    ("-c value", "run command"),
    ("--log value", "log file path"),
    ("--llv value", "log level"),
    ("--strict", "verify host key against ~/.ssh/known_hosts"),
    ("--help, -h", "show help"),
    ("--version, -v", "print the version"),
];

fn print_main_help() {
    let p = prog_name();
    println!("NAME:\n   assh-rs - Assh (Rust rewrite) - An SSH Client\n");
    println!("USAGE:\n   {p} [global options] command [command options] [arguments...]\n");
    println!("VERSION:\n   {VERSION}\n");
    println!("COMMANDS:");
    for (name, desc) in COMMAND_LIST {
        println!("   {:<9}  {}", name, desc);
    }
    println!("\nGLOBAL OPTIONS:");
    for (name, desc) in GLOBAL_OPTIONS {
        println!("   {:<13}  {}", name, desc);
    }
}

fn print_command_help(cmd: command_meta::Meta) {
    let p = prog_name();
    println!("NAME:\n   {p} {} - {}\n", cmd.name, cmd.usage);
    if cmd.options.is_empty() {
        println!("USAGE:\n   {p} {} [arguments...]\n", cmd.name);
    } else {
        println!(
            "USAGE:\n   {p} {} [command options] [arguments...]\n",
            cmd.name
        );
        println!("OPTIONS:");
        for (name, usage) in cmd.options {
            println!("   {:<8}  {}", name, usage);
        }
        println!("   ");
    }
}

mod command_meta {
    pub struct Meta {
        pub name: &'static str,
        pub usage: &'static str,
        pub options: &'static [(&'static str, &'static str)],
    }
}

fn meta_for(c: Command) -> command_meta::Meta {
    use Command::*;
    const NONE: &[(&str, &str)] = &[];
    const COMMON: &[(&str, &str)] = &[
        ("-H value", "server host"),
        ("-p value", "bind address port (default: 0)"),
        ("-l value", "login name"),
        ("-P value", "login password"),
        ("-k value", "automatic login public key"),
    ];
    const SET: &[(&str, &str)] = &[
        ("-H value", "server host"),
        ("-p value", "bind address port (default: 0)"),
        ("-l value", "login name"),
        ("-P value", "login password"),
        ("-k value", "automatic login public key"),
        ("-R value", "Server remark"),
        ("-f", "force set, don't tip"),
    ];
    const RM: &[(&str, &str)] = &[("-f", "force delete, don't tip"), ("-g", "delete group")];
    const PUSH: &[(&str, &str)] = &[(
        "-b value",
        "write up to BYTES bytes at a time (default: 1048576 = 1M)",
    )];
    const BC: &[(&str, &str)] = &[("-f value", "the Bash script file")];
    const IMPORT: &[(&str, &str)] = &[("-f", "force import, don't tip")];
    match c {
        Login => command_meta::Meta {
            name: "login",
            usage: "login server or exec remote server command",
            options: COMMON,
        },
        Set => command_meta::Meta {
            name: "set",
            usage: "Add/Modify the server",
            options: SET,
        },
        Rm => command_meta::Meta {
            name: "rm",
            usage: "Remove Server",
            options: RM,
        },
        Push => command_meta::Meta {
            name: "push",
            usage: "sftp push file to remote server",
            options: PUSH,
        },
        Pull => command_meta::Meta {
            name: "pull",
            usage: "sftp pull file from remote server",
            options: NONE,
        },
        Bc => command_meta::Meta {
            name: "bc",
            usage: "batch remote command execution",
            options: BC,
        },
        Import => command_meta::Meta {
            name: "import",
            usage: "import servers from YAML",
            options: IMPORT,
        },
        Account => command_meta::Meta {
            name: "account",
            usage: "Set the data crypto key (password)",
            options: NONE,
        },
        Ls => command_meta::Meta {
            name: "ls",
            usage: "Print all servers list",
            options: NONE,
        },
        Search => command_meta::Meta {
            name: "search",
            usage: "Search the server name",
            options: NONE,
        },
        Info => command_meta::Meta {
            name: "info",
            usage: "Print the server information",
            options: NONE,
        },
        Mv => command_meta::Meta {
            name: "mv",
            usage: "Move/Rename Server Name",
            options: NONE,
        },
        Export => command_meta::Meta {
            name: "export",
            usage: "export servers to YAML (plaintext)",
            options: NONE,
        },
        Ping => command_meta::Meta {
            name: "ping",
            usage: "ping all servers",
            options: NONE,
        },
        Version => command_meta::Meta {
            name: "version",
            usage: "print the version",
            options: NONE,
        },
        _ => command_meta::Meta {
            name: "",
            usage: "",
            options: NONE,
        },
    }
}

fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix('~')
        && let Some(home) = std::env::var_os("HOME")
    {
        return format!("{}{}", home.to_string_lossy(), rest);
    }
    p.to_string()
}

async fn run_connection(s: &Server, run_cmd: Option<String>, strict: bool) -> anyhow::Result<i32> {
    let hostport = format!("{}@{}:{}", s.user, s.host, s.port);
    let passwd_flag = if !s.key.is_empty() {
        "(pemKey: yes)".to_string()
    } else if !s.password.is_empty() {
        "(password: yes)".to_string()
    } else {
        "(password: no)".to_string()
    };
    println!("> connection: {} {} ", hostport, passwd_flag);

    let mut auth = Auth {
        user: s.user.clone(),
        strict,
        ..Default::default()
    };
    if !s.key.is_empty() {
        let pass = if s.password.is_empty() {
            None
        } else {
            Some(s.password.clone())
        };
        auth.key = Some((expand_tilde(&s.key), pass));
    }
    if !s.password.is_empty() {
        auth.password = Some(s.password.clone());
    }

    let start = Instant::now();
    let mut code = 0i32;
    let mut err: Option<anyhow::Error> = None;
    match run_cmd {
        Some(cmd) => {
            let r = async {
                let client =
                    SshClient::connect(&s.host, s.port, &auth, Duration::from_secs(30)).await?;
                let res = client.exec(&cmd).await?;
                client.disconnect().await.ok();
                Ok::<_, anyhow::Error>(res)
            }
            .await;
            let combined = match &r {
                Ok(res) => {
                    code = res.exit_status.unwrap_or(0) as i32;
                    String::from_utf8_lossy(&res.combined).into_owned()
                }
                Err(e) => {
                    err = Some(anyhow::anyhow!(e.to_string()));
                    String::new()
                }
            };
            // 对齐 oracle：即使连接失败，cmd 模式下仍打印 Result 块
            println!("\n> Result ({} -> '{}'): \n{}", s.name, hostport, combined);
        }
        None => match async {
            let client =
                SshClient::connect(&s.host, s.port, &auth, Duration::from_secs(30)).await?;
            let st = client.shell().await?;
            client.disconnect().await.ok();
            Ok::<_, anyhow::Error>(st)
        }
        .await
        {
            Ok(st) => code = st.unwrap_or(0) as i32,
            Err(e) => err = Some(e),
        },
    }
    println!(">  time cost: {:?}", start.elapsed());
    match err {
        Some(e) => Err(e),
        None => Ok(code),
    }
}

fn conn_flags_set(inv: &Invocation) -> bool {
    inv.server.is_some()
        || inv.flags.host.is_some()
        || inv.flags.port.is_some()
        || inv.flags.login.is_some()
        || inv.flags.password.is_some()
        || inv.flags.key.is_some()
}

#[tokio::main]
async fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let inv = match cli::parse(&argv) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Incorrect Usage: {e}");
            std::process::exit(1);
        }
    };

    if inv.flags.version {
        println!("{VERSION}");
        return;
    }
    if inv.flags.help || inv.command == Some(Command::Help) {
        match inv.command {
            Some(c) if c != Command::Help => {
                let m = meta_for(c);
                if m.name.is_empty() {
                    print_main_help();
                } else {
                    print_command_help(m);
                }
            }
            _ => print_main_help(),
        }
        return;
    }

    let code = run(&inv).await;
    std::process::exit(code);
}

async fn run(inv: &Invocation) -> i32 {
    match inv.command {
        Some(Command::Version) => {
            println!("{VERSION}");
            0
        }
        Some(Command::Ls) => with_store(|st| {
            cmds::cmd_ls(st);
            0
        }),
        Some(Command::Search) => with_store(|st| {
            cmds::cmd_search(st, inv.args.first());
            0
        }),
        Some(Command::Info) => with_store(|st| {
            cmds::cmd_info(st, inv.args.first());
            0
        }),
        Some(Command::Set) => store_result(|st| cmds::cmd_set(st, inv).map(|_| 0)),
        Some(Command::Mv) => store_result(|st| cmds::cmd_mv(st, inv).map(|_| 0)),
        Some(Command::Rm) => store_result(|st| cmds::cmd_rm(st, inv).map(|_| 0)),
        Some(Command::Account) => store_result(|st| cmds::cmd_account(st, inv).map(|_| 0)),
        Some(Command::Export) => {
            let path = inv
                .args
                .first()
                .cloned()
                .unwrap_or_else(|| "./output.yml".to_string());
            let path = expand_tilde(&path);
            store_result(|st| cmds::cmd_export(st, &path).map(|_| 0))
        }
        Some(Command::Import) => {
            let path = inv
                .args
                .first()
                .cloned()
                .unwrap_or_else(|| "./servers.yml".to_string());
            let path = expand_tilde(&path);
            store_result(|st| cmds::cmd_import(st, &path, inv.flags.force).map(|_| 0))
        }
        Some(Command::Ping) => match Store::open_default() {
            Ok(st) => {
                cmds::cmd_ping(&st, inv.flags.strict).await;
                0
            }
            Err(e) => {
                eprintln!("Assh: {e}");
                1
            }
        },
        Some(Command::Push) => {
            let store = Store::open_default().ok();
            let s = match cmds::resolve_server(store.as_ref(), inv) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Assh: {e}");
                    return 1;
                }
            };
            let (local, remote) = if inv.args.len() > 2 {
                (
                    inv.args[1..inv.args.len() - 1].to_vec(),
                    inv.args.last().unwrap().clone(),
                )
            } else {
                (inv.args.iter().skip(1).cloned().collect(), "~/".to_string())
            };
            let buf = inv
                .flags
                .block
                .filter(|b| *b > 0)
                .map(|b| b as usize)
                .unwrap_or(assh::sftp::DEFAULT_BUF);
            match cmds::cmd_push(&s, &local, &remote, buf, inv.flags.strict).await {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("Assh: sftp push fail: {e}");
                    1
                }
            }
        }
        Some(Command::Pull) => {
            let store = Store::open_default().ok();
            let s = match cmds::resolve_server(store.as_ref(), inv) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Assh: {e}");
                    return 1;
                }
            };
            let (remote, local) = if inv.args.len() > 2 {
                (
                    inv.args[1..inv.args.len() - 1].to_vec(),
                    inv.args.last().unwrap().clone(),
                )
            } else {
                (inv.args.iter().skip(1).cloned().collect(), "./".to_string())
            };
            match cmds::cmd_pull(&s, &remote, &local, inv.flags.strict).await {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("Assh: {e}");
                    1
                }
            }
        }
        Some(Command::Bc) => {
            let store = match Store::open_default() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Assh: {e}");
                    return 1;
                }
            };
            let name = inv.args.first().cloned().unwrap_or_default();
            let mut cmd = inv.args.get(1).cloned().unwrap_or_default();
            if let Some(f) = &inv.flags.file
                && let Ok(c) = std::fs::read_to_string(f)
            {
                cmd = c;
            }
            if name.is_empty() || cmd.is_empty() {
                return 0;
            }
            let members: Vec<Server> = if let Some(g) = store.get_group(&name) {
                g.values().cloned().collect()
            } else if let Some(s) = store.get(&name) {
                vec![s.clone()]
            } else {
                vec![]
            };
            let start = Instant::now();
            let strict = inv.flags.strict;
            let mut handles = vec![];
            for mut s in members {
                let c = cmd.clone();
                s.name = format!("{}@{}:{}", s.user, s.host, s.port);
                let nm = s.name.clone();
                handles.push(tokio::spawn(async move {
                    let r = cmds::exec_once(&s, &c, strict).await;
                    eprintln!("connection closed. {nm}");
                    r
                }));
            }
            for h in handles {
                h.await.ok();
            }
            println!("> BATCH COMMAND:  time cost: {:?}", start.elapsed());
            0
        }
        None | Some(Command::Login) => {
            if inv.command.is_none() && !conn_flags_set(inv) {
                print_main_help();
                return 0;
            }
            let store = Store::open_default().ok();
            let s = match cmds::resolve_server(store.as_ref(), inv) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Assh: {e}");
                    return 1;
                }
            };
            match run_connection(&s, inv.flags.run_cmd.clone(), inv.flags.strict).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Assh: {e}");
                    1
                }
            }
        }
        Some(c) => {
            eprintln!("command '{}' 未实现（Phase 6 范围外）", c.name());
            1
        }
    }
}

fn with_store(f: impl FnOnce(&Store) -> i32) -> i32 {
    match Store::open_default() {
        Ok(st) => f(&st),
        Err(e) => {
            eprintln!("Assh: {e}");
            1
        }
    }
}

fn store_result(f: impl FnOnce(&mut Store) -> anyhow::Result<i32>) -> i32 {
    match Store::open_default() {
        Ok(mut st) => match f(&mut st) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Assh: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("Assh: {e}");
            1
        }
    }
}
