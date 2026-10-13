//! SFTP push/pull。路径规则对齐 AUDIT §7：
//!   - 本地路径展开 `~`
//!   - 远端路径首字符 `~` 替换为 `.`（相对 sftp 初始目录）
//!   - 目录递归；直接覆盖已存在文件
//!   - `-b` 缓冲区大小（默认 1 MiB），支持 >1M 文件分块
use anyhow::{Context, Result, bail};
use russh_sftp::client::SftpSession;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const DEFAULT_BUF: usize = 1024 * 1024;

/// 等价 asshc.getRemoteRealPath
pub fn remote_real_path(remote: &str) -> String {
    if let Some(rest) = remote.strip_prefix('~') {
        format!(".{rest}")
    } else {
        remote.to_string()
    }
}

/// 等价 kiris.RealPath（仅展开 ~ 前缀）
pub fn local_real_path(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix('~')
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest.trim_start_matches('/'));
    }
    PathBuf::from(p)
}

pub async fn push_path(
    sftp: &SftpSession,
    local: &str,
    remote_dir: &str,
    buf_size: usize,
) -> Result<()> {
    let local = local_real_path(local);
    let remote_dir = remote_real_path(remote_dir);
    let name = local
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .context("无效本地路径")?;
    let remote = format!("{}/{}", remote_dir.trim_end_matches('/'), name);
    if local.is_dir() {
        push_dir(sftp, &local, &remote, buf_size).await
    } else {
        push_file(sftp, &local, &remote, buf_size).await
    }
}

pub async fn push_file(sftp: &SftpSession, local: &Path, remote: &str, buf: usize) -> Result<()> {
    let started = std::time::Instant::now();
    let r = push_file_inner(sftp, local, remote, buf).await;
    // 对齐 oracle：每文件一条 time cost（目录时每文件各一条）
    println!("\n time cost: {:?} ", started.elapsed());
    r
}

async fn push_file_inner(sftp: &SftpSession, local: &Path, remote: &str, buf: usize) -> Result<()> {
    let mut src = tokio::fs::File::open(local)
        .await
        .with_context(|| format!("打开本地文件失败: {}", local.display()))?;
    let mut dst = sftp
        .create(remote)
        .await
        .with_context(|| format!("创建远端文件失败: {remote}"))?;
    let mut buffer = vec![0u8; buf.max(1)];
    loop {
        let n = src.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        dst.write_all(&buffer[..n]).await?;
    }
    dst.flush().await?;
    dst.shutdown().await?;
    Ok(())
}

pub async fn push_dir(sftp: &SftpSession, local: &Path, remote: &str, buf: usize) -> Result<()> {
    // 与 AUDIT B-007 的原版缺陷不同：这里对每个子目录创建正确的目标路径
    let _ = sftp.create_dir(remote).await;
    let mut rd = tokio::fs::read_dir(local).await?;
    while let Some(entry) = rd.next_entry().await? {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let r = format!("{}/{}", remote.trim_end_matches('/'), name);
        if p.is_dir() {
            Box::pin(push_dir(sftp, &p, &r, buf)).await?;
        } else {
            push_file(sftp, &p, &r, buf).await?;
        }
    }
    Ok(())
}

pub async fn pull_path(sftp: &SftpSession, remote: &str, local: &str) -> Result<()> {
    let remote = remote_real_path(remote);
    let meta = sftp
        .metadata(remote.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Assh: remote file not found! {e}"))?;
    let base = Path::new(&remote)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| remote.clone());
    let local_path = Path::new(local).join(base);
    if meta.is_dir() {
        pull_dir(sftp, &remote, &local_path).await
    } else {
        pull_file(sftp, &remote, &local_path).await
    }
}

pub async fn pull_file(sftp: &SftpSession, remote: &str, local: &Path) -> Result<()> {
    let started = std::time::Instant::now();
    let r = pull_file_inner(sftp, remote, local).await;
    // 对齐 oracle：成功时每文件一条 time cost
    if r.is_ok() {
        println!("\n time cost: {:?} ", started.elapsed());
    }
    r
}

async fn pull_file_inner(sftp: &SftpSession, remote: &str, local: &Path) -> Result<()> {
    let mut src = sftp.open(remote).await?;
    if let Some(parent) = local.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let mut dst = tokio::fs::File::create(local).await?;
    let mut buffer = vec![0u8; 10240];
    loop {
        let n = src.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        dst.write_all(&buffer[..n]).await?;
    }
    dst.flush().await?;
    Ok(())
}

pub async fn pull_dir(sftp: &SftpSession, remote: &str, local: &Path) -> Result<()> {
    tokio::fs::create_dir_all(local).await.ok();
    let entries = sftp.read_dir(remote).await?;
    for entry in entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let r = format!("{}/{}", remote.trim_end_matches('/'), name);
        let l = local.join(&name);
        if entry.file_type().is_dir() {
            Box::pin(pull_dir(sftp, &r, &l)).await?;
        } else {
            pull_file(sftp, &r, &l).await?;
        }
    }
    Ok(())
}

pub fn ensure_buf(b: Option<i64>) -> usize {
    match b {
        Some(v) if v > 0 => v as usize,
        _ => DEFAULT_BUF,
    }
}

#[allow(dead_code)]
fn _unused() -> Result<()> {
    bail!("")
}
