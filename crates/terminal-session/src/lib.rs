use anyhow::{Context, Result};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
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

        let set_nonblock = || -> Result<()> {
            let flags =
                OFlag::from_bits_truncate(fcntl(master_fd, FcntlArg::F_GETFL).context("F_GETFL")?);
            let mut new_flags = flags;
            new_flags.insert(OFlag::O_NONBLOCK);
            fcntl(master_fd, FcntlArg::F_SETFL(new_flags)).context("F_SETFL")?;
            Ok(())
        };

        set_nonblock()?;

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

    pub fn snapshot(&self) -> TerminalBufferSnapshot {
        self.buffer.snapshot()
    }

    pub fn snapshot_text(&self) -> String {
        self.snapshot().to_string()
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
            'm' => self.buffer.apply_sgr(params),
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
    cells: Vec<TerminalCell>,
    attrs: TextAttributes,
}

impl TerminalBuffer {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cursor_row: 0,
            cursor_col: 0,
            cells: vec![TerminalCell::default(); rows * cols],
            attrs: TextAttributes::default(),
        }
    }

    fn index(&self, row: usize, col: usize) -> usize {
        row * self.cols + col
    }

    fn set_cell(&mut self, row: usize, col: usize, cell: TerminalCell) {
        if row < self.rows && col < self.cols {
            let idx = self.index(row, col);
            self.cells[idx] = cell;
        }
    }

    fn print(&mut self, ch: char) {
        let cell = TerminalCell::from_char(ch, self.attrs.fg, self.attrs.bg);
        self.set_cell(self.cursor_row, self.cursor_col, cell);
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
        self.cells.fill(TerminalCell::default());
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.attrs = TextAttributes::default();
    }

    fn scroll_up(&mut self) {
        let line = self.cols;
        self.cells.drain(0..line);
        self.cells
            .extend(std::iter::repeat(TerminalCell::default()).take(line));
    }

    fn snapshot(&self) -> TerminalBufferSnapshot {
        TerminalBufferSnapshot {
            rows: self.rows,
            cols: self.cols,
            cells: self.cells.clone(),
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        if params.is_empty() {
            self.attrs = TextAttributes::default();
            return;
        }
        let mut iter = params.iter().flat_map(|p| p.iter().copied());
        while let Some(code) = iter.next() {
            match code {
                0 => self.attrs = TextAttributes::default(),
                30..=37 => self.attrs.fg = ansi_color((code - 30) as u8, false),
                40..=47 => self.attrs.bg = ansi_color((code - 40) as u8, false),
                90..=97 => self.attrs.fg = ansi_color((code - 90) as u8, true),
                100..=107 => self.attrs.bg = ansi_color((code - 100) as u8, true),
                38 => {
                    if let Some(color) = read_extended_color(&mut iter) {
                        self.attrs.fg = color;
                    }
                }
                48 => {
                    if let Some(color) = read_extended_color(&mut iter) {
                        self.attrs.bg = color;
                    }
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalBufferSnapshot {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<TerminalCell>,
}

impl TerminalBufferSnapshot {
    pub fn cell(&self, row: usize, col: usize) -> Option<TerminalCell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        let idx = row * self.cols + col;
        self.cells.get(idx).copied()
    }
}

impl ToString for TerminalBufferSnapshot {
    fn to_string(&self) -> String {
        self.cells.iter().map(|cell| cell.ch).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalCell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl TerminalCell {
    fn from_char(ch: char, fg: Rgb, bg: Rgb) -> Self {
        Self { ch, fg, bg }
    }
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

const DEFAULT_FG: Rgb = Rgb::new(0xE6, 0xE6, 0xE6);
const DEFAULT_BG: Rgb = Rgb::new(0x00, 0x00, 0x00);

#[derive(Clone, Copy, Debug)]
struct TextAttributes {
    fg: Rgb,
    bg: Rgb,
}

impl Default for TextAttributes {
    fn default() -> Self {
        Self {
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        }
    }
}

fn ansi_color(index: u8, bright: bool) -> Rgb {
    let palette = [
        Rgb::new(0x00, 0x00, 0x00),
        Rgb::new(0xAA, 0x00, 0x00),
        Rgb::new(0x00, 0xAA, 0x00),
        Rgb::new(0xAA, 0x55, 0x00),
        Rgb::new(0x00, 0x00, 0xAA),
        Rgb::new(0xAA, 0x00, 0xAA),
        Rgb::new(0x00, 0xAA, 0xAA),
        Rgb::new(0xAA, 0xAA, 0xAA),
    ];
    let bright_palette = [
        Rgb::new(0x55, 0x55, 0x55),
        Rgb::new(0xFF, 0x55, 0x55),
        Rgb::new(0x55, 0xFF, 0x55),
        Rgb::new(0xFF, 0xFF, 0x55),
        Rgb::new(0x55, 0x55, 0xFF),
        Rgb::new(0xFF, 0x55, 0xFF),
        Rgb::new(0x55, 0xFF, 0xFF),
        Rgb::new(0xFF, 0xFF, 0xFF),
    ];
    let palette_index = index.min(7) as usize;
    if bright {
        bright_palette[palette_index]
    } else {
        palette[palette_index]
    }
}

fn read_extended_color<I>(iter: &mut I) -> Option<Rgb>
where
    I: Iterator<Item = u16>,
{
    let mode = iter.next()?;
    match mode {
        2 => {
            let r = iter.next()? as u8;
            let g = iter.next()? as u8;
            let b = iter.next()? as u8;
            Some(Rgb::new(r, g, b))
        }
        _ => None,
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

    #[test]
    fn buffer_snapshot_exposes_grid() {
        let mut buffer = TerminalBuffer::new(2, 3);
        buffer.print('A');
        buffer.newline();
        buffer.print('B');
        let snap = buffer.snapshot();
        assert_eq!(snap.rows, 2);
        assert_eq!(snap.cols, 3);
        assert_eq!(snap.cell(0, 0).map(|c| c.ch), Some('A'));
        assert_eq!(snap.cell(1, 0).map(|c| c.ch), Some('B'));
    }

    #[test]
    fn sgr_truecolor_updates_cell_color() {
        let mut buffer = TerminalBuffer::new(1, 2);
        let mut parser = Parser::new();
        {
            let mut emulator = Emulator {
                buffer: &mut buffer,
            };
            parser.advance(&mut emulator, b"\x1b[38;2;10;20;30mX");
        }
        let snap = buffer.snapshot();
        let cell = snap.cell(0, 0).expect("cell");
        assert_eq!(cell.ch, 'X');
        assert_eq!(cell.fg, Rgb::new(10, 20, 30));
        assert_eq!(cell.bg, DEFAULT_BG);
    }

    #[test]
    fn colored_ascii_art_snapshot() {
        let mut buffer = TerminalBuffer::new(2, 3);
        let mut parser = Parser::new();
        {
            let mut emulator = Emulator {
                buffer: &mut buffer,
            };
            parser.advance(
                &mut emulator,
                b"\x1b[38;2;255;0;0mA\x1b[38;2;0;255;0mB\n\x1b[38;2;0;0;255mC\x1b[38;2;255;255;0mD",
            );
        }
        let snap = buffer.snapshot();
        let top_left = snap.cell(0, 0).unwrap();
        let top_right = snap.cell(0, 1).unwrap();
        let bottom_left = snap.cell(1, 0).unwrap();
        let bottom_right = snap.cell(1, 1).unwrap();
        assert_eq!(top_left.fg, Rgb::new(255, 0, 0));
        assert_eq!(top_right.fg, Rgb::new(0, 255, 0));
        assert_eq!(bottom_left.fg, Rgb::new(0, 0, 255));
        assert_eq!(bottom_right.fg, Rgb::new(255, 255, 0));
    }
}
