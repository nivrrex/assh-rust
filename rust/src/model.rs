use serde::{Deserialize, Serialize};

/// 与 asshc.Server 的 JSON 布局一致（字段顺序即 Go struct 声明顺序，
/// 用于 L3 字节级兼容）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Server {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub key: String,
    pub remark: String,
    pub options: Option<serde_json::Value>,
}

impl Server {
    pub fn with_defaults() -> Self {
        Server {
            user: "root".into(),
            port: 22,
            ..Default::default()
        }
    }

    /// 与 cmd/server.go parseHostString 一致：user@host:port
    pub fn parse_host_string(s: &str) -> (String, String, u16) {
        let mut rest = s;
        let mut user = String::new();
        if let Some(i) = rest.find('@')
            && i > 0
        {
            user = rest[..i].to_string();
            rest = &rest[i + 1..];
        }
        let mut port = 0u16;
        if let Some(i) = rest.find(':') {
            if let Ok(p) = rest[i + 1..].parse::<u16>()
                && p > 0
            {
                port = p;
            }
            rest = &rest[..i];
        }
        (rest.to_string(), user, port)
    }
}

/// 分组/短名解析，等价 asshc.parseName。
pub fn parse_name(name: &str) -> (String, String) {
    match name.splitn(2, '.').collect::<Vec<_>>().as_slice() {
        [only] => (String::new(), only.to_string()),
        [g, s] => (g.to_string(), s.to_string()),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_string() {
        assert_eq!(
            Server::parse_host_string("root@h:2222"),
            ("h".into(), "root".into(), 2222)
        );
        assert_eq!(Server::parse_host_string("h"), ("h".into(), "".into(), 0));
        assert_eq!(
            Server::parse_host_string("u@h"),
            ("h".into(), "u".into(), 0)
        );
    }

    #[test]
    fn names() {
        assert_eq!(parse_name("a"), ("".into(), "a".into()));
        assert_eq!(parse_name("g.s"), ("g".into(), "s".into()));
    }
}
