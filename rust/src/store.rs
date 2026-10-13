//! 数据层：~/.assh 的读写。格式见 docs/AUDIT.md §1。
//!
//!   ~/.assh/.rsa        PKCS#1 RSA 私钥（PEM "RSA PRIVATE KEY"）
//!   ~/.assh/.rsa.pub    PKIX 公钥（PEM "PUBLIC KEY"）
//!   ~/.assh/.account    RSA_PKCS1v15( MD5(master) )
//!   ~/.assh/assh.yml    RSA_PKCS1v15( YAML )
//!   ~/.assh/data/servers.db
//!       MD5(passwd||ct) || MD5(ct) || ct
//!       ct = AES-256-CBC(PKCS7(JSON), key=passwd, iv=passwd[..16])
//!       passwd = MD5(master) 的 32 字符小写十六进制
//!
//! 写路径使用“临时文件 + rename”**原子替换**，修复原版 B-004（O_WRONLY 不截断导致库损坏）。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
use md5::{Digest, Md5};
use rsa::pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey};
use rsa::pkcs8::{DecodePublicKey, EncodePublicKey};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};

use crate::model::Server;

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

/// 等价 Go 的 os/user.Current().HomeDir（不依赖 $HOME）。
pub fn home_dir() -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        let dir = (*pw).pw_dir;
        if dir.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(dir).to_string_lossy().into_owned())
    }
}

pub fn md5_hex(data: &[u8]) -> String {
    let mut h = Md5::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub type Data = BTreeMap<String, BTreeMap<String, Server>>;

pub struct Store {
    pub basedir: PathBuf,
    pub data: Data,
    passwd: Vec<u8>,
    pub db_path: PathBuf,
}

impl Store {
    pub fn default_dir() -> Result<PathBuf> {
        let home = home_dir().context("无法确定 home 目录")?;
        Ok(PathBuf::from(home).join(".assh"))
    }

    /// 首次运行初始化（对齐上游 asshc/config.go 的 init()）：
    /// 建 ~/.assh、生成 .rsa/.rsa.pub、写默认 assh.yml、建数据目录。幂等：已存在的文件不改动。
    pub fn ensure_layout(basedir: &Path) -> Result<()> {
        std::fs::create_dir_all(basedir)
            .with_context(|| format!("创建目录失败: {}", basedir.display()))?;
        let priv_p = basedir.join(".rsa");
        let pub_p = basedir.join(".rsa.pub");
        // 典型误操作：手工 mkdir 出 .rsa/.rsa.pub，给出可操作的提示
        for p in [&priv_p, &pub_p] {
            if p.is_dir() {
                bail!(
                    "{} 是目录，应为文件；请删除 {} 后重试（assh 会自动初始化）",
                    p.display(),
                    basedir.display()
                );
            }
        }
        // 与原版一致：两者都不存在才生成；只缺一个视为库损坏，交给后续读取报错。
        if !priv_p.exists() && !pub_p.exists() {
            let (priv_pem, pub_pem) = gen_rsa_pem()?;
            atomic_write(&pub_p, pub_pem.as_bytes())?;
            atomic_write(&priv_p, priv_pem.as_bytes())?;
            set_owner_only(&priv_p)?;
        }
        let yaml_p = basedir.join("assh.yml");
        if !yaml_p.exists() {
            let pub_pem = read(&pub_p)?;
            let ct = rsa_encrypt(&pub_pem, default_yaml(basedir).as_bytes())?;
            atomic_write(&yaml_p, &ct)?;
        }
        let data_dir = db_dir(&read(&priv_p)?, &read(&yaml_p)?, basedir)?;
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("创建数据目录失败: {}", data_dir.display()))?;
        Ok(())
    }

    /// 打开一个已有库；`.account` 可缺失（首次运行，passwd 为空）。
    pub fn open(basedir: &Path) -> Result<Self> {
        let priv_pem = read(basedir.join(".rsa"))?;
        let passwd = match read_opt(&basedir.join(".account"))? {
            Some(account) => rsa_decrypt(&priv_pem, &account)?,
            None => Vec::new(),
        };
        let yaml_ct = read(basedir.join("assh.yml"))?;
        let db_path = db_dir(&priv_pem, &yaml_ct, basedir)?.join("servers.db");
        let data = if db_path.exists() {
            if passwd.len() != 32 {
                bail!(
                    "{} 存在，但缺少 {}，无法解密；请先执行 assh account 设置主口令",
                    db_path.display(),
                    basedir.join(".account").display()
                );
            }
            let blob = read(&db_path)?;
            let plain = decrypt_db(&blob, &passwd)?;
            serde_json::from_slice(&plain).unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        Ok(Store {
            basedir: basedir.to_path_buf(),
            data,
            passwd,
            db_path,
        })
    }

    /// 初始化（如缺失）+ 打开默认库。
    pub fn open_default() -> Result<Self> {
        let dir = Self::default_dir()?;
        Self::ensure_layout(&dir)?;
        Self::open(&dir)
    }

    /// 用主口令打开/初始化。`master` 用于校验 .account。
    pub fn master_matches(&self, master: &str) -> bool {
        md5_hex(master.as_bytes()).as_bytes() == self.passwd.as_slice()
    }

    pub fn save(&self) -> Result<()> {
        if self.passwd.len() != 32 {
            bail!(
                "尚未设置主口令（缺少 {}），请先执行 assh account",
                self.basedir.join(".account").display()
            );
        }
        let json = serde_json::to_vec(&self.data)?;
        let blob = encrypt_db(&json, &self.passwd);
        atomic_write(&self.db_path, &blob)
    }

    // ---- 服务器操作（对齐 asshc.Assh 语义）----
    pub fn list(&self) -> &Data {
        &self.data
    }

    pub fn get(&self, name: &str) -> Option<&Server> {
        let (g, s) = crate::model::parse_name(name);
        self.data.get(&g).and_then(|m| m.get(&s))
    }

    pub fn get_group(&self, group: &str) -> Option<&BTreeMap<String, Server>> {
        self.data.get(group)
    }

    pub fn set_server(&mut self, name: &str, mut server: Server) {
        if server.password.is_empty() && server.key.is_empty() {
            server.key = "~/.ssh/id_rsa".to_string();
        }
        if !server.key.is_empty()
            && let Some(dst) = copy_pem_key(&self.basedir, &self.db_path, &server)
        {
            server.key = dst;
        }
        server.name = name.to_string();
        let (g, s) = crate::model::parse_name(name);
        self.data.entry(g).or_default().insert(s, server);
    }

    pub fn del(&mut self, name: &str) -> Result<bool> {
        let (g, s) = crate::model::parse_name(name);
        let existed = self
            .data
            .get_mut(&g)
            .map(|m| m.remove(&s).is_some())
            .unwrap_or(false);
        if existed {
            del_pem_key(&self.db_path, name);
            if let Some(m) = self.data.get(&g)
                && m.is_empty()
            {
                self.data.remove(&g);
            }
        }
        Ok(existed)
    }

    pub fn move_server(&mut self, from: &str, to: &str) -> Result<()> {
        // 对齐原版：目标已存在时不报错而直接覆盖（log.Fatalf 在 OFF 下为 no-op）
        let s = self
            .get(from)
            .cloned()
            .context(format!("Server [{from}] does not exist."))?;
        self.set_server(to, s);
        self.del(from)?;
        Ok(())
    }

    /// 内部 passwd(MD5 十六进制) 字符串副本，用于口令校验。
    pub fn passwd_hex(&self) -> String {
        String::from_utf8_lossy(&self.passwd).into_owned()
    }

    /// account：设置/修改主口令（全库重加密）。
    pub fn change_master(&mut self, new_master: &str) -> Result<()> {
        if new_master.is_empty() {
            bail!("The password is empty");
        }
        let pub_pem = read(self.basedir.join(".rsa.pub"))?;
        let new_passwd = md5_hex(new_master.as_bytes());
        // 备份旧 .account
        let account = self.basedir.join(".account");
        let had_account = account.exists();
        if had_account {
            std::fs::copy(&account, account.with_file_name(".account-old")).ok();
        }
        // 用新 key 重加密 DB；与原版一致：首次设置口令（此前无 .account）时不写 servers.db
        self.passwd = new_passwd.clone().into_bytes();
        if had_account && self.db_path.exists() {
            self.save()?;
        }
        // 写新 .account
        let ct = rsa_encrypt(&pub_pem, new_passwd.as_bytes())?;
        atomic_write(&account, &ct)?;
        Ok(())
    }

    pub fn search(&self, q: &str) -> Data {
        let q = q.to_lowercase();
        let mut out = Data::new();
        for (g, servers) in &self.data {
            for (n, s) in servers {
                let hit = s.name.to_lowercase().contains(&q)
                    || s.host.contains(&q)
                    || s.remark.contains(&q);
                if hit {
                    out.entry(g.clone())
                        .or_default()
                        .insert(n.clone(), s.clone());
                }
            }
        }
        out
    }
}

// ---------------- 加密原语 ----------------

pub fn decrypt_db(blob: &[u8], passwd: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < 64 {
        bail!("servers.db 过短（{} 字节）", blob.len());
    }
    let (head_pw, head_data, ct) = (&blob[..32], &blob[32..64], &blob[64..]);
    if md5_hex(ct).as_bytes() != head_data {
        bail!("The data signature has been modified.");
    }
    if md5_hex(&[passwd, ct].concat()).as_bytes() != head_pw {
        bail!("Access denied for password. run assh account [your password]");
    }
    let key: [u8; 32] = passwd.try_into().context("口令必须为 32 字节")?;
    let iv: [u8; 16] = passwd[..16].try_into().unwrap();
    let dec = Aes256CbcDec::new(&key.into(), &iv.into());
    dec.decrypt_padded_vec::<Pkcs7>(ct)
        .map_err(|e| anyhow::anyhow!("AES 解密失败: {e}"))
}

pub fn encrypt_db(plain: &[u8], passwd: &[u8]) -> Vec<u8> {
    assert_eq!(passwd.len(), 32, "passwd 必须是 32 字节 MD5 十六进制");
    let key: [u8; 32] = passwd.try_into().unwrap();
    let iv: [u8; 16] = passwd[..16].try_into().unwrap();
    let enc = Aes256CbcEnc::new(&key.into(), &iv.into());
    let ct = enc.encrypt_padded_vec::<Pkcs7>(plain);
    let mut out = Vec::with_capacity(64 + ct.len());
    out.extend_from_slice(md5_hex(&[passwd, &ct].concat()).as_bytes());
    out.extend_from_slice(md5_hex(&ct).as_bytes());
    out.extend_from_slice(&ct);
    out
}

fn rsa_decrypt(priv_pem: &[u8], ct: &[u8]) -> Result<Vec<u8>> {
    let pem = std::str::from_utf8(priv_pem)?;
    let key = RsaPrivateKey::from_pkcs1_pem(pem)?;
    Ok(key.decrypt(Pkcs1v15Encrypt, ct)?)
}

fn rsa_encrypt(pub_pem: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    let pem = std::str::from_utf8(pub_pem)?;
    let pubkey = RsaPublicKey::from_public_key_pem(pem)?;
    let mut rng = rsa::rand_core::OsRng;
    Ok(pubkey.encrypt(&mut rng, Pkcs1v15Encrypt, plain)?)
}

// ---------------- 文件/路径工具 ----------------

pub fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix('~')
        && let Some(home) = home_dir()
    {
        return format!("{home}{rest}");
    }
    p.to_string()
}

/// 默认配置内容（与上游 cnfDefaultContent 一致；dbPath 为绝对路径）。
fn default_yaml(basedir: &Path) -> String {
    format!(
        "logPath: ~/.assh/assh.log\nlogLevel: OFF\ndbPath: {}\nqiniuAccessKey: \nqiniuSecretKey:\nqiniuBucket:\n",
        basedir.join("data").display()
    )
}

/// 从 assh.yml 解出数据目录（默认 <basedir>/data）。
fn db_dir(priv_pem: &[u8], yaml_ct: &[u8], basedir: &Path) -> Result<PathBuf> {
    let yaml_plain = rsa_decrypt(priv_pem, yaml_ct)?;
    let yaml: serde_yaml::Value = serde_yaml::from_slice(&yaml_plain).unwrap_or_default();
    Ok(yaml
        .get("dbPath")
        .and_then(|v| v.as_str())
        .map(expand_tilde)
        .map(PathBuf::from)
        .unwrap_or_else(|| basedir.join("data")))
}

/// RSA-2048 密钥对：私钥 PKCS#1 PEM、公钥 PKIX PEM（与 keygen.NewRsa 一致）。
fn gen_rsa_pem() -> Result<(String, String)> {
    let mut rng = rsa::rand_core::OsRng;
    let key = RsaPrivateKey::new(&mut rng, 2048)?;
    let priv_pem = key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)?.to_string();
    let pub_pem = key
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)?;
    Ok((priv_pem, pub_pem))
}

fn set_owner_only(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 失败: {}", p.display()))
}

fn read(p: impl AsRef<Path>) -> Result<Vec<u8>> {
    let p = p.as_ref();
    if p.is_dir() {
        bail!("读取失败: {} 是目录，应为文件", p.display());
    }
    std::fs::read(p).with_context(|| format!("读取失败: {}", p.display()))
}

/// 读文件；不存在返回 None（其它错误照常报错）。
fn read_opt(p: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(p) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("读取失败: {}", p.display())),
    }
}

/// 原子写：写临时文件后 rename，保证不残留旧尾部（修复 B-004）。
pub fn atomic_write(p: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, data).with_context(|| format!("写失败: {}", tmp.display()))?;
    std::fs::rename(&tmp, p).with_context(|| format!("rename 失败: {}", p.display()))?;
    Ok(())
}

/// 等价 setPemKey：把私钥复制到 data/<serverName>/<basename> 并返回新路径。
fn copy_pem_key(basedir: &Path, db_path: &Path, s: &Server) -> Option<String> {
    let src = PathBuf::from(expand_tilde(&s.key));
    if !src.exists() {
        return None;
    }
    let data_dir = db_path.parent().unwrap_or(basedir);
    let server_dir = data_dir.join(s.name.replace('.', "/"));
    std::fs::create_dir_all(&server_dir).ok();
    let base = src.file_name()?;
    let dst = server_dir.join(base);
    std::fs::copy(&src, &dst).ok()?;
    let src_pub = PathBuf::from(format!("{}.pub", src.display()));
    if src_pub.exists() {
        std::fs::copy(&src_pub, format!("{}.pub", dst.display())).ok();
    }
    Some(dst.to_string_lossy().into_owned())
}

fn del_pem_key(db_path: &Path, name: &str) {
    if let Some(data_dir) = db_path.parent() {
        let p = data_dir.join(name.replace('.', "/"));
        if p.exists() {
            std::fs::remove_dir_all(&p).ok();
        }
    }
}

#[allow(dead_code)]
fn _unused_keyparts() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_known() {
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn db_roundtrip() {
        let passwd = md5_hex(b"Master+Pass_1").into_bytes();
        let plain = br#"{"":{"a":{"name":"a","host":"h","port":22,"user":"u","password":"p","key":"","remark":"","options":null}}}"#;
        let blob = encrypt_db(plain, &passwd);
        assert_eq!(blob.len() % 16, 0);
        assert_eq!(decrypt_db(&blob, &passwd).unwrap(), plain);
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("assh_store_test_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// 首次运行：初始化 → 空口令打开 → 首次设密 → 重开校验 → set+save → 读回。
    #[test]
    fn init_layout_then_account_roundtrip() {
        let dir = tmp_dir("init");
        Store::ensure_layout(&dir).unwrap();
        assert!(dir.join(".rsa").is_file());
        assert!(dir.join(".rsa.pub").is_file());
        assert!(dir.join("assh.yml").is_file());
        assert!(dir.join("data").is_dir());
        // 首次：.account 缺失，passwd 为空
        let mut st = Store::open(&dir).unwrap();
        assert!(st.passwd_hex().is_empty());
        st.change_master("Master+Pass_1").unwrap();
        assert!(dir.join(".account").is_file());
        // 首次设密不应产生 servers.db
        assert!(!dir.join("data/servers.db").exists());
        // 重开：口令校验
        let mut st = Store::open(&dir).unwrap();
        assert!(st.master_matches("Master+Pass_1"));
        st.set_server("s1", crate::model::Server::with_defaults());
        st.save().unwrap();
        let st = Store::open(&dir).unwrap();
        assert!(st.get("s1").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 未设口令时 save() 必须返回 Err（不能 panic）。
    #[test]
    fn save_without_account_errors() {
        let dir = tmp_dir("nosave");
        Store::ensure_layout(&dir).unwrap();
        let st = Store::open(&dir).unwrap();
        assert!(st.save().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
