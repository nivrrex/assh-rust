//! 手写 CLI 解析层。
//!
//! 目标（对照 AUDIT §2 与 Phase 3.3）：
//!   - 识别全部子命令
//!   - 全局 flag 位置灵活：`assh login x -H y` 与 `assh -H y login x` **等价**（差异 BD-001）
//!   - 未知首参数 → 服务器名/主机串（直连）
//!   - `-c` 在命令前后均生效
//!   - 命令局部 flag 覆盖同名全局 flag（如 keygen 的 `-c` 是注释）
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Version,
    Account,
    Ls,
    Search,
    Info,
    Set,
    Mv,
    Rm,
    Login,
    Bc,
    Push,
    Pull,
    Keygen,
    Sync,
    Export,
    Import,
    Upgrade,
    Ping,
    Proxy,
    Hostproxy,
    Help,
}

impl Command {
    pub fn from_name(s: &str) -> Option<Command> {
        Some(match s {
            "version" => Command::Version,
            "account" => Command::Account,
            "ls" => Command::Ls,
            "search" => Command::Search,
            "info" => Command::Info,
            "set" => Command::Set,
            "mv" => Command::Mv,
            "rm" => Command::Rm,
            "login" => Command::Login,
            "bc" => Command::Bc,
            "push" => Command::Push,
            "pull" => Command::Pull,
            "keygen" => Command::Keygen,
            "sync" => Command::Sync,
            "export" => Command::Export,
            "import" => Command::Import,
            "upgrade" => Command::Upgrade,
            "ping" => Command::Ping,
            "proxy" => Command::Proxy,
            "hostproxy" => Command::Hostproxy,
            "help" | "h" => Command::Help,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Command::Version => "version",
            Command::Account => "account",
            Command::Ls => "ls",
            Command::Search => "search",
            Command::Info => "info",
            Command::Set => "set",
            Command::Mv => "mv",
            Command::Rm => "rm",
            Command::Login => "login",
            Command::Bc => "bc",
            Command::Push => "push",
            Command::Pull => "pull",
            Command::Keygen => "keygen",
            Command::Sync => "sync",
            Command::Export => "export",
            Command::Import => "import",
            Command::Upgrade => "upgrade",
            Command::Ping => "ping",
            Command::Proxy => "proxy",
            Command::Hostproxy => "hostproxy",
            Command::Help => "help",
        }
    }

    pub fn local_flags(&self) -> &'static [FlagSpec] {
        use Command::*;
        const NONE: &[FlagSpec] = &[];
        const F_BOOL: &[FlagSpec] = &[FlagSpec::bool('f', None)];
        const COMMON: &[FlagSpec] = &[
            FlagSpec::val('H', None),
            FlagSpec::val('p', None),
            FlagSpec::val('l', None),
            FlagSpec::val('P', None),
            FlagSpec::val('k', None),
        ];
        const SET: &[FlagSpec] = &[
            FlagSpec::val('H', None),
            FlagSpec::val('p', None),
            FlagSpec::val('l', None),
            FlagSpec::val('P', None),
            FlagSpec::val('k', None),
            FlagSpec::val('R', Some("remark")),
            FlagSpec::bool('f', None),
        ];
        const RM: &[FlagSpec] = &[FlagSpec::bool('f', None), FlagSpec::bool('g', None)];
        const PUSH: &[FlagSpec] = &[FlagSpec::val('b', None)];
        const KEYGEN: &[FlagSpec] = &[FlagSpec::val('c', None), FlagSpec::val('f', None)];
        const BC: &[FlagSpec] = &[FlagSpec::val('f', None)];
        const SYNC: &[FlagSpec] = &[FlagSpec::bool('d', None)];
        const PROXY: &[FlagSpec] = &[FlagSpec::val('d', None), FlagSpec::val('i', None)];
        const HOSTPROXY: &[FlagSpec] = &[
            FlagSpec::val('H', None),
            FlagSpec::val('P', None),
            FlagSpec::val('d', None),
            FlagSpec::val('i', None),
        ];
        match self {
            Set => SET,
            Rm => RM,
            Login => COMMON,
            Push => PUSH,
            Keygen => KEYGEN,
            Bc => BC,
            Import => F_BOOL,
            Upgrade => F_BOOL,
            Sync => SYNC,
            Proxy => PROXY,
            Hostproxy => HOSTPROXY,
            _ => NONE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FlagSpec {
    pub short: Option<char>,
    pub long: Option<&'static str>,
    pub takes_value: bool,
}

impl FlagSpec {
    pub const fn val(short: char, long: Option<&'static str>) -> Self {
        FlagSpec {
            short: Some(short),
            long,
            takes_value: true,
        }
    }
    pub const fn bool(short: char, long: Option<&'static str>) -> Self {
        FlagSpec {
            short: Some(short),
            long,
            takes_value: false,
        }
    }
    /// 仅长选项的布尔 flag（如 --strict）
    pub const fn long_bool(long: &'static str) -> Self {
        FlagSpec {
            short: None,
            long: Some(long),
            takes_value: false,
        }
    }
}

const GLOBALS: &[FlagSpec] = &[
    FlagSpec::val('H', None),
    FlagSpec::val('p', None),
    FlagSpec::val('l', None),
    FlagSpec::val('P', None),
    FlagSpec::val('k', None),
    FlagSpec::val('c', None),
    FlagSpec::val('x', Some("log")),
    FlagSpec::val('x', Some("llv")),
    FlagSpec::bool('h', Some("help")),
    FlagSpec::bool('v', Some("version")),
    FlagSpec::long_bool("strict"),
];

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Flags {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub login: Option<String>,
    pub password: Option<String>,
    pub key: Option<String>,
    pub run_cmd: Option<String>,
    pub log: Option<String>,
    pub llv: Option<String>,
    pub remark: Option<String>,
    pub force: bool,
    pub group: bool,
    pub block: Option<i64>,
    pub file: Option<String>,
    pub comment: Option<String>,
    pub date: bool,
    pub d: Option<String>,
    pub i: Option<String>,
    pub help: bool,
    pub version: bool,
    pub strict: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    pub command: Option<Command>,
    /// 直连服务器名/主机串（当 command 为 None）
    pub server: Option<String>,
    /// 位置参数（含服务器名）
    pub args: Vec<String>,
    pub flags: Flags,
}

#[derive(Debug, PartialEq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseError {}

fn lookup<'a>(
    specs: &'a [FlagSpec],
    short: Option<char>,
    long: Option<&str>,
) -> Option<&'a FlagSpec> {
    specs.iter().find(|s| match (short, long) {
        (Some(c), _) => s.short == Some(c),
        (None, Some(l)) => s.long == Some(l),
        _ => false,
    })
}

/// 判断 token 是否为“取值型 flag”，用于 pass1 跳过其值。
fn global_takes_value(token: &str) -> bool {
    let (short, long) = split_flag(token);
    lookup(GLOBALS, short, long)
        .map(|s| s.takes_value)
        .unwrap_or(false)
}

fn split_flag(token: &str) -> (Option<char>, Option<&str>) {
    if let Some(rest) = token.strip_prefix("--") {
        let name = rest.split('=').next().unwrap_or(rest);
        (None, Some(name))
    } else if let Some(rest) = token.strip_prefix('-') {
        (rest.chars().next(), None)
    } else {
        (None, None)
    }
}

pub fn parse(argv: &[String]) -> Result<Invocation, ParseError> {
    // pass 1: 找命令（首个非 flag 位置参数）
    let mut command = None;
    let mut cmd_index = None;
    let mut i = 0;
    while i < argv.len() {
        let t = &argv[i];
        if t == "--" {
            if i + 1 < argv.len() {
                // 下一个 token 是位置参数
                let cand = &argv[i + 1];
                if Command::from_name(cand).is_some() {
                    command = Command::from_name(cand);
                    cmd_index = Some(i + 1);
                }
            }
            break;
        }
        if t.starts_with('-') && t != "-" {
            if global_takes_value(t) && !t.contains('=') {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        command = Command::from_name(t);
        if command.is_some() {
            cmd_index = Some(i);
        }
        break;
    }

    // pass 2: 解析所有 flag 与位置参数
    let mut specs: Vec<FlagSpec> = GLOBALS.to_vec();
    if let Some(c) = command {
        for s in c.local_flags() {
            specs.push(*s);
        }
    }

    let mut flags = Flags::default();
    let mut positionals: Vec<String> = Vec::new();
    i = 0;
    while i < argv.len() {
        let t = argv[i].clone();
        if Some(i) == cmd_index {
            i += 1;
            continue;
        }
        if t == "--" {
            positionals.extend_from_slice(&argv[i + 1..]);
            break;
        }
        if t.starts_with('-') && t != "-" {
            let (short, long) = split_flag(&t);
            let inline = t.split_once('=').map(|(_, v)| v.to_string());
            let spec = lookup(&specs, short, long).copied();
            match spec {
                None => return Err(ParseError(format!("flag provided but not defined: {}", t))),
                Some(sp) => {
                    let value = if sp.takes_value {
                        if let Some(v) = inline {
                            Some(v)
                        } else {
                            i += 1;
                            argv.get(i).cloned()
                        }
                    } else {
                        if inline.is_some() {
                            return Err(ParseError(format!("flag does not take value: {}", t)));
                        }
                        None
                    };
                    assign(&mut flags, command, &sp, value, &t)?;
                }
            }
            i += 1;
            continue;
        }
        positionals.push(t);
        i += 1;
    }

    let (server, args) = if command.is_some() {
        (None, positionals)
    } else {
        if positionals.is_empty() {
            (None, vec![])
        } else {
            (Some(positionals[0].clone()), positionals.clone())
        }
    };

    Ok(Invocation {
        command,
        server,
        args,
        flags,
    })
}

fn assign(
    flags: &mut Flags,
    command: Option<Command>,
    spec: &FlagSpec,
    value: Option<String>,
    token: &str,
) -> Result<(), ParseError> {
    let need = || ParseError(format!("flag needs an argument: {}", token));
    match (spec.short, spec.long) {
        (Some('H'), _) => flags.host = Some(value.ok_or_else(need)?),
        (Some('p'), _) => {
            let v = value.ok_or_else(need)?;
            flags.port = Some(
                v.parse()
                    .map_err(|_| ParseError(format!("invalid port: {v}")))?,
            );
        }
        (Some('l'), _) => flags.login = Some(value.ok_or_else(need)?),
        (Some('P'), _) => flags.password = Some(value.ok_or_else(need)?),
        (Some('k'), _) => flags.key = Some(value.ok_or_else(need)?),
        (Some('c'), None) => {
            if command == Some(Command::Keygen) {
                flags.comment = Some(value.ok_or_else(need)?);
            } else {
                flags.run_cmd = Some(value.ok_or_else(need)?);
            }
        }
        (_, Some("log")) => flags.log = Some(value.ok_or_else(need)?),
        (_, Some("llv")) => flags.llv = Some(value.ok_or_else(need)?),
        (Some('R'), _) => flags.remark = Some(value.ok_or_else(need)?),
        (Some('f'), None) => {
            if value.is_some() {
                flags.file = value;
            } else {
                flags.force = true;
            }
        }
        (Some('g'), _) => flags.group = true,
        (Some('b'), _) => {
            let v = value.ok_or_else(need)?;
            flags.block = Some(
                v.parse()
                    .map_err(|_| ParseError(format!("invalid -b: {v}")))?,
            );
        }
        (Some('d'), _) => {
            if value.is_some() {
                flags.d = value;
            } else {
                flags.date = true;
            }
        }
        (Some('i'), _) => flags.i = Some(value.ok_or_else(need)?),
        (Some('h'), _) => flags.help = true,
        (Some('v'), _) => flags.version = true,
        (_, Some("strict")) => flags.strict = true,
        _ => return Err(ParseError(format!("unhandled flag: {}", token))),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use Command::*;

    fn p(args: &[&str]) -> Invocation {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse(&v).unwrap()
    }

    #[test]
    fn direct_connect_server_name() {
        let inv = p(&["srv-a"]);
        assert_eq!(inv.command, None);
        assert_eq!(inv.server.as_deref(), Some("srv-a"));
    }

    #[test]
    fn direct_connect_with_flags_anywhere() {
        let a = p(&["srv-a", "-c", "echo hi"]);
        assert_eq!(a.command, None);
        assert_eq!(a.server.as_deref(), Some("srv-a"));
        assert_eq!(a.flags.run_cmd.as_deref(), Some("echo hi"));

        let b = p(&["-c", "echo hi", "srv-a"]);
        assert_eq!(b.server.as_deref(), Some("srv-a"));
        assert_eq!(b.flags.run_cmd.as_deref(), Some("echo hi"));
    }

    #[test]
    fn unknown_first_arg_is_server() {
        let inv = p(&["not-a-command", "-H", "1.2.3.4"]);
        assert_eq!(inv.command, None);
        assert_eq!(inv.server.as_deref(), Some("not-a-command"));
        assert_eq!(inv.flags.host.as_deref(), Some("1.2.3.4"));
    }

    #[test]
    fn commands_recognized() {
        for (s, c) in [
            ("ls", Ls),
            ("set", Set),
            ("rm", Rm),
            ("login", Login),
            ("push", Push),
            ("pull", Pull),
            ("keygen", Keygen),
            ("export", Export),
            ("import", Import),
            ("ping", Ping),
            ("proxy", Proxy),
            ("hostproxy", Hostproxy),
            ("bc", Bc),
            ("version", Version),
            ("account", Account),
            ("sync", Sync),
        ] {
            assert_eq!(p(&[s]).command, Some(c), "cmd {s}");
        }
    }

    /// 核心：两种书写顺序必须等价（BD-001）
    #[test]
    fn global_flags_before_and_after_command_are_equivalent() {
        let after = p(&[
            "login", "srv-a", "-H", "1.2.3.4", "-p", "2222", "-l", "u", "-P", "pw",
        ]);
        let before = p(&[
            "-H", "1.2.3.4", "-p", "2222", "-l", "u", "-P", "pw", "login", "srv-a",
        ]);
        assert_eq!(after.command, Some(Login));
        assert_eq!(before.command, Some(Login));
        assert_eq!(after.flags, before.flags);
        assert_eq!(after.args, before.args);
    }

    #[test]
    fn command_c_after_command_is_run_cmd() {
        let inv = p(&["login", "srv-a", "-c", "echo A"]);
        assert_eq!(inv.command, Some(Login));
        assert_eq!(inv.flags.run_cmd.as_deref(), Some("echo A"));
    }

    #[test]
    fn command_c_before_command_is_run_cmd() {
        let inv = p(&["-c", "echo A", "login", "srv-a"]);
        assert_eq!(inv.command, Some(Login));
        assert_eq!(inv.flags.run_cmd.as_deref(), Some("echo A"));
    }

    #[test]
    fn keygen_c_is_comment_and_f_is_string() {
        let inv = p(&["keygen", "-c", "me@x", "-f", "./id_rsa"]);
        assert_eq!(inv.command, Some(Keygen));
        assert_eq!(inv.flags.comment.as_deref(), Some("me@x"));
        assert_eq!(inv.flags.file.as_deref(), Some("./id_rsa"));
        assert!(!inv.flags.force);
    }

    #[test]
    fn set_f_is_bool() {
        let inv = p(&["set", "s", "-H", "h", "-f"]);
        assert_eq!(inv.command, Some(Set));
        assert!(inv.flags.force);
        assert_eq!(inv.flags.host.as_deref(), Some("h"));
    }

    #[test]
    fn log_and_llv_long_flags() {
        let inv = p(&["ls", "--log", "/tmp/a.log", "--llv", "DEBUG"]);
        assert_eq!(inv.flags.log.as_deref(), Some("/tmp/a.log"));
        assert_eq!(inv.flags.llv.as_deref(), Some("DEBUG"));
    }

    #[test]
    fn equals_form() {
        let inv = p(&["login", "s", "-H=1.2.3.4", "-p=22"]);
        assert_eq!(inv.flags.host.as_deref(), Some("1.2.3.4"));
        assert_eq!(inv.flags.port, Some(22));
    }

    #[test]
    fn no_args() {
        let inv = p(&[]);
        assert_eq!(inv.command, None);
        assert_eq!(inv.server, None);
    }

    #[test]
    fn version_and_help_flags() {
        assert!(p(&["--version"]).flags.version);
        assert!(p(&["-v"]).flags.version);
        assert!(p(&["--help"]).flags.help);
        assert!(p(&["-h"]).flags.help);
        assert_eq!(p(&["--version"]).command, None);
    }

    #[test]
    fn sync_push_d_bool_and_proxy_d_value() {
        let s = p(&["sync", "push", "-d"]);
        assert_eq!(s.command, Some(Sync));
        assert!(s.flags.date);
        assert_eq!(s.args, vec!["push".to_string()]);

        let x = p(&["proxy", "srv", "-d", "1080", "-i", "127.0.0.1"]);
        assert_eq!(x.flags.d.as_deref(), Some("1080"));
        assert_eq!(x.flags.i.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn unknown_flag_is_error() {
        let v: Vec<String> = ["ls", "-Z"].iter().map(|s| s.to_string()).collect();
        assert!(parse(&v).is_err());
    }

    #[test]
    fn push_b_value() {
        let inv = p(&["push", "srv", "-b", "4096", "/tmp/f"]);
        assert_eq!(inv.flags.block, Some(4096));
        assert_eq!(inv.args, vec!["srv".to_string(), "/tmp/f".to_string()]);
    }
}
