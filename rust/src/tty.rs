//! 终端 raw mode 守卫（初版：快照 + Drop 恢复 + 尺寸查询）。
//! Phase 7 将补信号处理与幂等恢复。
use anyhow::Result;
use nix::sys::termios::{self, SetArg, Termios};
use std::os::fd::{BorrowedFd, RawFd};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WinSize {
    pub cols: u32,
    pub rows: u32,
}

pub fn terminal_size(fd: RawFd) -> WinSize {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) };
    if rc == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
        WinSize {
            cols: ws.ws_col as u32,
            rows: ws.ws_row as u32,
        }
    } else {
        WinSize { cols: 80, rows: 24 }
    }
}

pub struct TerminalGuard {
    fd: RawFd,
    original: Option<Termios>,
}

impl TerminalGuard {
    /// 进入 raw 模式，返回守卫；守卫 drop 时恢复。
    pub fn enter_raw(fd: RawFd) -> Result<Self> {
        let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
        let original = termios::tcgetattr(borrowed)?;
        let mut raw = original.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(borrowed, SetArg::TCSANOW, &raw)?;
        Ok(Self {
            fd,
            original: Some(original),
        })
    }

    /// 幂等恢复；可安全多次调用。
    pub fn restore(&mut self) {
        if let Some(orig) = self.original.take() {
            let borrowed = unsafe { BorrowedFd::borrow_raw(self.fd) };
            let _ = termios::tcsetattr(borrowed, SetArg::TCSANOW, &orig);
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_is_idempotent() {
        // 用 /dev/null 不可 tcgetattr，直接验证多次 restore 不 panic（original=None 路径）
        let mut g = TerminalGuard {
            fd: 0,
            original: None,
        };
        g.restore();
        g.restore();
    }
}
