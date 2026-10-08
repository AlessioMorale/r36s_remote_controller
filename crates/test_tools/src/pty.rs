//! A pty pair standing in for the TX module's UART: the daemon opens `slave_path`, the
//! fake module reads and writes the master.

use anyhow::{Context, Result};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::pty::openpty;
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, SetArg};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::path::PathBuf;

pub struct Pty {
    master: OwnedFd,
    /// Kept open so the master never reports EIO while the daemon is reconnecting
    _slave: OwnedFd,
    pub slave_path: PathBuf,
}

impl Pty {
    pub fn open() -> Result<Self> {
        let pty = openpty(None, None).context("openpty")?;
        let mut termios = tcgetattr(&pty.slave).context("tcgetattr")?;
        cfmakeraw(&mut termios);
        tcsetattr(&pty.slave, SetArg::TCSANOW, &termios).context("tcsetattr")?;
        fcntl(pty.master.as_raw_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).context("O_NONBLOCK")?;
        let slave_path = nix::unistd::ttyname(&pty.slave).context("ttyname")?;
        Ok(Self { master: pty.master, _slave: pty.slave, slave_path })
    }

    /// Bytes the daemon has written so far (empty if none)
    pub fn read_available(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            match nix::unistd::read(self.master.as_raw_fd(), &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => out.extend_from_slice(&buffer[..n]),
            }
        }
        out
    }

    pub fn write(&self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            match nix::unistd::write(self.master.as_fd(), bytes) {
                Ok(0) | Err(_) => break,
                Ok(n) => bytes = &bytes[n..],
            }
        }
    }
}
