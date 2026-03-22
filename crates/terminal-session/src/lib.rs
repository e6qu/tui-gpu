use anyhow::{Context, Result};
use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag};
use nix::unistd::{close, dup2, execvp, fork, setsid, ForkResult, Pid};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};

pub struct TerminalSession {
    master: File,
    child: Pid,
}

impl TerminalSession {
    pub fn spawn(command: &str, cols: u16, rows: u16) -> Result<Self> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = openpty(Some(&winsize), None).context("openpty failed")?;
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();

        match unsafe { fork().context("fork failed")? } {
            ForkResult::Parent { child } => {
                close(slave_fd).ok();
                let master_file = unsafe { File::from_raw_fd(master_fd) };
                Ok(Self {
                    master: master_file,
                    child,
                })
            }
            ForkResult::Child => {
                if let Err(err) = child_exec(master_fd, slave_fd, command) {
                    eprintln!("terminal child exec error: {err:?}");
                }
                unsafe { libc::_exit(1) };
            }
        }
    }

    pub fn read_all(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.master
            .read_to_end(&mut buf)
            .context("read master pty failed")?;
        Ok(buf)
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.master
            .write_all(data)
            .context("write master pty failed")
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = kill(self.child, Signal::SIGHUP);
        let _ = waitpid(self.child, Some(WaitPidFlag::WNOHANG));
    }
}

fn child_exec(master: RawFd, slave: RawFd, command: &str) -> Result<()> {
    close(master).ok();
    setsid().context("setsid failed")?;
    unsafe {
        if libc::ioctl(slave, libc::TIOCSCTTY.into(), 0) != 0 {
            return Err(anyhow::anyhow!("TIOCSCTTY failed"));
        }
    }
    dup2(slave, libc::STDIN_FILENO).context("dup2 stdin failed")?;
    dup2(slave, libc::STDOUT_FILENO).context("dup2 stdout failed")?;
    dup2(slave, libc::STDERR_FILENO).context("dup2 stderr failed")?;
    close(slave).ok();

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell_c = CString::new(shell).unwrap();
    let arg_c = CString::new("-c").unwrap();
    let cmd_c = CString::new(command).unwrap();
    let args = [shell_c.as_c_str(), arg_c.as_c_str(), cmd_c.as_c_str()];
    execvp(&shell_c, &args).context("execvp failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn spawn_and_read_output() {
        let mut session = TerminalSession::spawn("printf test", 80, 24).unwrap();
        thread::sleep(Duration::from_millis(100));
        let buf = session.read_all().unwrap();
        assert!(String::from_utf8_lossy(&buf).contains("test"));
    }
}
