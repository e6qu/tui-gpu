use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use ansi_image::{convert_image_to_ansi, DEFAULT_CELL_ASPECT, DEFAULT_PALETTE};
use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use crossterm::{
    cursor,
    event::{
        self, Event as TermEvent, KeyCode as TermKeyCode, KeyEvent as TermKeyEvent,
        KeyEventKind as TermKeyEventKind, KeyModifiers as TermKeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute, queue,
    terminal::{self, ClearType},
};
use frame_feed::{
    AudioFeedReader, FrameFeedReader, InputFeedWriter, DEFAULT_AUDIO_CAPACITY, DEFAULT_AUDIO_PATH,
    DEFAULT_FEED_CAPACITY, DEFAULT_FEED_PATH, DEFAULT_INPUT_CAPACITY, DEFAULT_INPUT_PATH,
};
use image::{DynamicImage, ImageBuffer, Rgb};
use rodio::{buffer::SamplesBuffer, OutputStream, OutputStreamHandle, Sink};

use doom_input::{self, KeySymbol};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long)]
    frame_feed: Option<String>,
    #[arg(long)]
    input_feed: Option<String>,
    #[arg(long)]
    audio_feed: Option<String>,
    /// Automatically spawn doom-feed with the given IWAD path
    #[arg(long, value_name = "IWAD")]
    doom: Option<PathBuf>,
    /// Extra arguments forwarded to doom-feed
    #[arg(long, value_name = "ARG", requires = "doom", action = clap::ArgAction::Append)]
    doom_arg: Vec<String>,
}

struct FeedPaths {
    frame: String,
    input: String,
    audio: String,
}

impl FeedPaths {
    fn resolve(cli: &Cli) -> Self {
        Self {
            frame: cli
                .frame_feed
                .clone()
                .unwrap_or_else(default_frame_feed_path),
            input: cli
                .input_feed
                .clone()
                .unwrap_or_else(default_input_feed_path),
            audio: cli
                .audio_feed
                .clone()
                .unwrap_or_else(default_audio_feed_path),
        }
    }
}

struct AudioPlayer {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl AudioPlayer {
    fn start(path: &str, wait_for_writer: bool) -> Option<Self> {
        let reader = match open_audio_reader_with_retry(path, wait_for_writer) {
            Ok(reader) => reader,
            Err(err) => {
                eprintln!("audio feed unavailable: {err:?}");
                return None;
            }
        };
        let (stream, handle) = OutputStream::try_default().ok()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let handle_clone = handle.clone();
        let thread = thread::spawn(move || run_audio_loop(reader, handle_clone, stop_clone));
        Some(Self {
            _stream: stream,
            _handle: handle,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let feeds = FeedPaths::resolve(&cli);
    let mut stdout = io::stdout();
    let _guard = TuiGuard::enter(&mut stdout)?;
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    let mut reader = open_reader_with_retry(&feeds.frame, cli.doom.is_some())?;
    let mut input_writer = match InputFeedWriter::open(&feeds.input, DEFAULT_INPUT_CAPACITY) {
        Ok(writer) => Some(writer),
        Err(err) => {
            eprintln!("input feed unavailable (keyboard disabled): {err:?}");
            None
        }
    };
    if cli.doom.is_some() && input_writer.is_none() {
        bail!("failed to open input feed writer at {}", feeds.input);
    }
    let _audio_player = AudioPlayer::start(&feeds.audio, cli.doom.is_some());
    let doom_child = if let Some(iwad) = cli.doom.as_ref() {
        Some(spawn_doom_process(iwad, &cli.doom_arg, &feeds)?)
    } else {
        None
    };
    let palette: Vec<char> = DEFAULT_PALETTE.chars().collect();
    loop {
        if event::poll(Duration::from_millis(5))? {
            if let TermEvent::Key(key) = event::read()? {
                if should_exit_key(key) {
                    break;
                }
                let pressed =
                    matches!(key.kind, TermKeyEventKind::Press | TermKeyEventKind::Repeat);
                if let Some(writer) = input_writer.as_mut() {
                    if let Some(doom_key) = doom_key_from_term_key(&key) {
                        let _ = writer.write_event(&[if pressed { 1 } else { 0 }, doom_key]);
                        continue;
                    }
                }
                if let Some(bytes) = key_event_to_bytes(key) {
                    if let Some(writer) = input_writer.as_mut() {
                        let _ = writer.write_event(&bytes);
                    }
                }
            }
        }
        match reader.poll_frame() {
            Ok(Some(frame)) => {
                let (term_cols, term_rows) = terminal::size()?;
                if term_cols == 0 || term_rows == 0 {
                    thread::sleep(Duration::from_millis(16));
                    continue;
                }
                render_frame(
                    &mut stdout,
                    &palette,
                    term_cols,
                    term_rows,
                    frame.width,
                    frame.height,
                    frame.pixels,
                )?;
            }
            Ok(None) => thread::sleep(Duration::from_millis(16)),
            Err(err) => {
                eprintln!("frame feed read error: {err:?}");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    stdout.flush()?;
    drop(doom_child);
    Ok(())
}

fn default_frame_feed_path() -> String {
    std::env::var("TUI_GPU_FRAME_FEED").unwrap_or_else(|_| DEFAULT_FEED_PATH.to_string())
}

fn default_input_feed_path() -> String {
    std::env::var("TUI_GPU_INPUT_FEED").unwrap_or_else(|_| DEFAULT_INPUT_PATH.to_string())
}

fn default_audio_feed_path() -> String {
    std::env::var("TUI_GPU_AUDIO_FEED").unwrap_or_else(|_| DEFAULT_AUDIO_PATH.to_string())
}

fn term_key_symbol(key: &TermKeyEvent) -> Option<KeySymbol> {
    use TermKeyCode::*;
    match key.code {
        Char(c) => Some(KeySymbol::Char(c)),
        Up => Some(KeySymbol::ArrowUp),
        Down => Some(KeySymbol::ArrowDown),
        Left => Some(KeySymbol::ArrowLeft),
        Right => Some(KeySymbol::ArrowRight),
        Esc => Some(KeySymbol::Escape),
        Enter => Some(KeySymbol::Enter),
        Tab => Some(KeySymbol::Tab),
        Backspace => Some(KeySymbol::Backspace),
        Home => Some(KeySymbol::Home),
        End => Some(KeySymbol::End),
        PageUp => Some(KeySymbol::PageUp),
        PageDown => Some(KeySymbol::PageDown),
        _ => None,
    }
}

fn doom_key_from_term_key(key: &TermKeyEvent) -> Option<u8> {
    term_key_symbol(key).and_then(doom_input::scancode_from_symbol)
}

fn fit_ansi_dimensions(frame_width: u32, frame_height: u32, cols: u16, rows: u16) -> (u32, u32) {
    let mut max_cols = cols.max(1) as u32;
    let max_rows = rows.max(1) as u32;
    let frame_width = frame_width.max(1);
    let frame_height = frame_height.max(1);
    let frame_ratio = frame_height as f32 / frame_width as f32;
    let height_per_col = frame_ratio * DEFAULT_CELL_ASPECT;
    if !height_per_col.is_finite() || height_per_col <= 0.0 {
        return (max_cols, max_rows.min(max_cols).max(1));
    }
    let max_cols_for_rows = (max_rows as f32 / height_per_col).floor().max(1.0) as u32;
    max_cols = max_cols.min(max_cols_for_rows).max(1);
    let mut target_rows = ((max_cols as f32 * height_per_col).round().max(1.0)) as u32;
    while target_rows > max_rows && max_cols > 1 {
        max_cols -= 1;
        target_rows = ((max_cols as f32 * height_per_col).round().max(1.0)) as u32;
    }
    target_rows = target_rows.min(max_rows).max(1);
    (max_cols, target_rows)
}

fn render_frame(
    stdout: &mut io::Stdout,
    palette: &[char],
    term_cols: u16,
    term_rows: u16,
    frame_width: u32,
    frame_height: u32,
    pixels: Vec<u8>,
) -> Result<()> {
    let (target_cols, target_rows) =
        fit_ansi_dimensions(frame_width, frame_height, term_cols, term_rows);
    let img = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(frame_width, frame_height, pixels)
        .ok_or_else(|| anyhow!("invalid frame"))?;
    let dyn_img = DynamicImage::ImageRgb8(img);
    let ansi = convert_image_to_ansi(
        &dyn_img,
        target_cols,
        Some(target_rows),
        palette,
        DEFAULT_CELL_ASPECT,
    )?;
    let used_cols = target_cols.min(term_cols as u32) as u16;
    let padding = term_cols.saturating_sub(used_cols) / 2;
    let lines: Vec<&str> = ansi.lines().collect();
    let used_rows = lines.len().min(term_rows as usize);
    for (row_idx, line) in lines.iter().take(used_rows).enumerate() {
        queue!(
            stdout,
            cursor::MoveTo(0, row_idx as u16),
            terminal::Clear(ClearType::CurrentLine),
            cursor::MoveTo(padding, row_idx as u16),
        )?;
        stdout.write_all(line.as_bytes())?;
    }
    for row in used_rows as u16..term_rows {
        queue!(
            stdout,
            cursor::MoveTo(0, row),
            terminal::Clear(ClearType::CurrentLine),
        )?;
    }
    stdout.flush()?;
    Ok(())
}

fn open_reader_with_retry(path: &str, wait_for_writer: bool) -> Result<FrameFeedReader> {
    let mut attempts = 0;
    loop {
        match FrameFeedReader::open(path, DEFAULT_FEED_CAPACITY) {
            Ok(reader) => return Ok(reader),
            Err(err) => {
                attempts += 1;
                if !wait_for_writer || attempts > 100 {
                    return Err(err);
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn open_audio_reader_with_retry(path: &str, wait_for_writer: bool) -> Result<AudioFeedReader> {
    let mut attempts = 0;
    loop {
        match AudioFeedReader::open(path, DEFAULT_AUDIO_CAPACITY) {
            Ok(reader) => return Ok(reader),
            Err(err) => {
                attempts += 1;
                if !wait_for_writer || attempts > 100 {
                    return Err(err);
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn should_exit_key(key: TermKeyEvent) -> bool {
    if key.modifiers.is_empty() && matches!(key.code, TermKeyCode::Char('q') | TermKeyCode::Esc) {
        return true;
    }
    if key.modifiers.contains(TermKeyModifiers::CONTROL)
        && matches!(key.code, TermKeyCode::Char('c') | TermKeyCode::Char('C'))
    {
        return true;
    }
    false
}

fn key_event_to_bytes(key: TermKeyEvent) -> Option<Vec<u8>> {
    use TermKeyCode::*;
    match key.code {
        Up => Some(b"\x1b[A".to_vec()),
        Down => Some(b"\x1b[B".to_vec()),
        Right => Some(b"\x1b[C".to_vec()),
        Left => Some(b"\x1b[D".to_vec()),
        Home => Some(b"\x1b[H".to_vec()),
        End => Some(b"\x1b[F".to_vec()),
        PageUp => Some(b"\x1b[5~".to_vec()),
        PageDown => Some(b"\x1b[6~".to_vec()),
        Insert => Some(b"\x1b[2~".to_vec()),
        Delete => Some(b"\x1b[3~".to_vec()),
        Enter => Some(b"\r".to_vec()),
        Backspace => Some(vec![0x7f]),
        Esc => Some(vec![0x1b]),
        Tab => Some(vec![b'\t']),
        Char(' ') => Some(vec![b' ']),
        Char(c) => Some(vec![c as u8]),
        _ => None,
    }
}

fn run_audio_loop(mut reader: AudioFeedReader, handle: OutputStreamHandle, stop: Arc<AtomicBool>) {
    let sink = match Sink::try_new(&handle) {
        Ok(sink) => sink,
        Err(err) => {
            eprintln!("failed to initialize audio sink: {err:?}");
            return;
        }
    };
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match reader.poll_event() {
            Ok(Some(event)) => {
                if let Some(buffer) = audio_buffer_from_event(&event) {
                    sink.append(buffer);
                }
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(err) => {
                eprintln!("audio feed read error: {err:?}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
    sink.stop();
}

fn audio_buffer_from_event(event: &frame_feed::AudioEvent) -> Option<SamplesBuffer<i16>> {
    if event.samples.is_empty() || event.sample_rate == 0 {
        return None;
    }
    let stereo = convert_audio_event(event);
    Some(SamplesBuffer::new(2, event.sample_rate, stereo))
}

fn convert_audio_event(event: &frame_feed::AudioEvent) -> Vec<i16> {
    use std::f32::consts::FRAC_PI_2;
    let volume = (event.volume as f32 / 127.0).clamp(0.0, 1.0);
    let pan = (event.separation as f32 / 255.0).clamp(0.0, 1.0);
    let angle = pan * FRAC_PI_2;
    let left_gain = angle.cos();
    let right_gain = angle.sin();
    let mut samples = Vec::with_capacity(event.samples.len() * 2);
    for &sample in &event.samples {
        let normalized = sample as f32 / 32768.0;
        let left = (normalized * volume * left_gain).clamp(-1.0, 1.0);
        let right = (normalized * volume * right_gain).clamp(-1.0, 1.0);
        samples.push((left * 32767.0) as i16);
        samples.push((right * 32767.0) as i16);
    }
    samples
}

fn spawn_doom_process(iwad: &Path, extra_args: &[String], feeds: &FeedPaths) -> Result<DoomChild> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.args(["run", "-p", "doom-feed", "--", "--", "-iwad"]);
    cmd.arg(iwad);
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.env("TUI_GPU_FRAME_FEED", &feeds.frame);
    cmd.env("TUI_GPU_INPUT_FEED", &feeds.input);
    cmd.env("TUI_GPU_AUDIO_FEED", &feeds.audio);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.stdin(Stdio::null());
    let child = cmd.spawn().context("failed to spawn doom-feed")?;
    Ok(DoomChild(child))
}

struct DoomChild(Child);

impl Drop for DoomChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

struct TuiGuard;

impl TuiGuard {
    fn enter(stdout: &mut io::Stdout) -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(
            stdout,
            cursor::Hide,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
        )?;
        Ok(Self)
    }
}

impl Drop for TuiGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, PopKeyboardEnhancementFlags, cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}
