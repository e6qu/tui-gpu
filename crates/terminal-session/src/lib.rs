use anyhow::{Context, Result};
use nix::pty::{openpty, Winsize};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag};
use nix::unistd::{close, dup2, execvp, fork, setsid, ForkResult, Pid};
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use vte::{Params, Parser, Perform};

const READ_CHUNK: usize = 4096;

pub struct TerminalSession {
    master: std::fs::File,
    child: Pid,
    parser: Parser,
    buffer: TerminalBuffer,
}

impl TerminalSession {
    pub fn spawn(command: &str, cols: usize, rows: usize) -> Result<Self> {
        let winsize = Winsize {
            ws_row: rows as u16,
            ws_col: cols as u16,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = openpty(Some(&winsize), None).context("openpty failed")?;
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();

        match unsafe { fork().context("fork failed")? } {
            ForkResult::Parent { child } => {
                close(slave_fd).ok();
                let master = unsafe { std::fs::File::from_raw_fd(master_fd) };
                Ok(Self {
                    master,
                    child,
                    parser: Parser::new(),
                    buffer: TerminalBuffer::new(rows, cols),
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

    pub fn process_output(&mut self) -> Result<()> {
        let mut buf = [0u8; READ_CHUNK];
        loop {
            match self.master.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut performer = Emulator {
                        buffer: &mut self.buffer,
                    };
                    self.parser.advance(&mut performer, &buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn snapshot_text(&self) -> String {
        self.buffer.to_string()
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

struct Emulator<'a> {
    buffer: &'a mut TerminalBuffer,
}

impl<'a> Perform for Emulator<'a> {
    fn print(&mut self, c: char) {
        self.buffer.print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => self.buffer.newline(),
            b'\r' => self.buffer.carriage_return(),
            b'\t' => self.buffer.tab(),
            0x08 => self.buffer.backspace(),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        match action {
            'H' | 'f' => {
                let row = params
                    .iter()
                    .next()
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                let col = params
                    .iter()
                    .nth(1)
                    .and_then(|p| p.first())
                    .copied()
                    .unwrap_or(1) as usize;
                self.buffer
                    .move_cursor(row.saturating_sub(1), col.saturating_sub(1));
            }
            'J' => self.buffer.clear(),
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalBuffer {
    rows: usize,
    cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    cells: Vec<char>,
}

impl TerminalBuffer {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cursor_row: 0,
            cursor_col: 0,
            cells: vec![' '; rows * cols],
        }
    }

    fn index(&self, row: usize, col: usize) -> usize {
        row * self.cols + col
    }

    fn set_cell(&mut self, row: usize, col: usize, ch: char) {
        if row < self.rows && col < self.cols {
            let idx = self.index(row, col);
            self.cells[idx] = ch;
        }
    }

    fn print(&mut self, ch: char) {
        self.set_cell(self.cursor_row, self.cursor_col, ch);
        self.cursor_col += 1;
        if self.cursor_col >= self.cols {
            self.newline();
        }
    }

    fn newline(&mut self) {
        self.cursor_col = 0;
        if self.cursor_row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.cursor_row += 1;
        }
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    fn tab(&mut self) {
        let next_tab = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next_tab.min(self.cols.saturating_sub(1));
    }

    fn move_cursor(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.rows.saturating_sub(1));
        self.cursor_col = col.min(self.cols.saturating_sub(1));
    }

    fn clear(&mut self) {
        self.cells.fill(' ');
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn scroll_up(&mut self) {
        let line = self.cols;
        self.cells.drain(0..line);
        self.cells.extend(std::iter::repeat(' ').take(line));
    }

    fn to_string(&self) -> String {
        self.cells.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn spawn_and_capture_text() {
        let mut session = TerminalSession::spawn("printf hello", 40, 5).unwrap();
        thread::sleep(Duration::from_millis(100));
        session.process_output().unwrap();
        assert!(session.snapshot_text().contains("hello"));
    }
}
