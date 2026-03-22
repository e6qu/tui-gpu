use ansi_image::{convert_image_to_blocks, DEFAULT_CELL_ASPECT};
use anyhow::{bail, Context, Result};
use crossterm::{
    cursor,
    event::{
        self as term_event, Event as TermEvent, KeyCode as TermKeyCode, KeyEvent as TermKeyEvent,
        KeyEventKind as TermKeyEventKind, KeyModifiers as TermKeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute, queue,
    style::{Color as TermColor, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self as term_terminal, ClearType as TermClearType},
};
use demos::{DoomFeedSource, PlasmaSource, RayDemoSource, RgbSource};
use doom_input::{self, KeySymbol};
use frame_feed::{
    AudioFeedReader, InputFeedWriter, DEFAULT_AUDIO_CAPACITY, DEFAULT_AUDIO_PATH,
    DEFAULT_INPUT_CAPACITY, DEFAULT_INPUT_PATH,
};
use gpu_demos::{create_gpu_demo, GpuDemo};
use image::{DynamicImage, RgbImage};
use layout::{ComponentKind, LayoutDocument, LayoutNode};
use log::{info, warn};
use rgb_frame::{FrameRenderer, FrameVertex, RgbFrame};
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::{
    env,
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use taffy::prelude::{
    AvailableSpace, Dimension, FlexDirection as TaffyFlexDirection, LengthPercentageAuto, Node,
    Position, Rect as TaffyRect, Size, Style, Taffy,
};
use terminal_session::{Rgb as TermRgb, TerminalBufferSnapshot, TerminalSession};
use text::{GlyphVertex, TextPipeline, CELL_PIXEL_HEIGHT, CELL_PIXEL_WIDTH};
use wgpu::util::DeviceExt;
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::WindowBuilder,
};
use youtube::{YoutubeSettings, YoutubeSource};

mod demos;
mod gpu_demos;
mod layout;
mod rgb_frame;
mod text;
mod youtube;

const MENU_BAR_HEIGHT: f32 = 48.0;
const OVERLAY_HEIGHT: f32 = 32.0;
const LAYOUT_PATH: &str = "generated/layouts/main_screen.json";
const TUI_FRAME_SLEEP_MS: u64 = 16;
const TUI_EVENT_POLL_MS: u64 = 5;
const RGB_DEMO_WIDTH: u32 = 320;
const RGB_DEMO_HEIGHT: u32 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoKind {
    Terminal,
    Plasma,
    Ray,
    Doom,
    Youtube,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppMode {
    Gui,
    Tui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputeMode {
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputFocus {
    Terminal,
    Feed,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RendererOptions {
    pub fps_sample_interval: Option<Duration>,
    pub youtube_audio: Option<bool>,
}

pub fn run_app(demo: DemoKind, compute: ComputeMode, mode: AppMode) -> Result<()> {
    run_app_with_options(demo, compute, mode, RendererOptions::default())
}

pub fn run_app_with_options(
    demo: DemoKind,
    compute: ComputeMode,
    mode: AppMode,
    opts: RendererOptions,
) -> Result<()> {
    let compute = sanitize_compute(demo, compute);
    match mode {
        AppMode::Gui => run_gui_with_options(demo, compute, opts),
        AppMode::Tui => run_tui_with_options(demo, compute, opts),
    }
}

pub fn run_gui(demo: DemoKind, compute: ComputeMode) -> Result<()> {
    run_gui_with_options(demo, compute, RendererOptions::default())
}

pub fn run_gui_with_options(
    demo: DemoKind,
    compute: ComputeMode,
    opts: RendererOptions,
) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("tui-gpu renderer")
            .build(&event_loop)?,
    );
    let mut state = pollster::block_on(State::new(window.clone(), demo, compute, opts))?;
    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);
        match event {
            Event::WindowEvent { event, window_id } if window_id == window.id() => match event {
                WindowEvent::CloseRequested => target.exit(),
                WindowEvent::Resized(size) => state.resize(size),
                WindowEvent::ScaleFactorChanged { .. } => {
                    state.resize(window.inner_size());
                }
                WindowEvent::KeyboardInput { event, .. } => state.handle_key(event, target),
                WindowEvent::RedrawRequested => {
                    if let Err(err) = state.render() {
                        info!("render error: {err:?}");
                    }
                }
                _ => {}
            },
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    })?;
    Ok(())
}

pub fn run_tui(demo: DemoKind, compute: ComputeMode) -> Result<()> {
    run_tui_with_options(demo, compute, RendererOptions::default())
}

pub fn run_tui_with_options(
    demo: DemoKind,
    compute: ComputeMode,
    opts: RendererOptions,
) -> Result<()> {
    match demo {
        DemoKind::Terminal => run_terminal_tui(),
        DemoKind::Plasma | DemoKind::Ray | DemoKind::Doom | DemoKind::Youtube => match compute {
            ComputeMode::Cpu => run_rgb_tui_cpu(demo, cpu_source_for(demo, &opts)?),
            ComputeMode::Gpu => run_rgb_tui_gpu(demo),
        },
    }
}

fn sanitize_compute(demo: DemoKind, requested: ComputeMode) -> ComputeMode {
    match demo {
        DemoKind::Plasma | DemoKind::Ray => requested,
        DemoKind::Terminal | DemoKind::Doom | DemoKind::Youtube => {
            if matches!(requested, ComputeMode::Gpu) {
                warn!("{demo:?} demo does not support GPU compute; using CPU");
                ComputeMode::Cpu
            } else {
                requested
            }
        }
    }
}

fn run_terminal_tui() -> Result<()> {
    let mut stdout = io::stdout();
    let _guard = TuiGuard::enter(&mut stdout)?;
    execute!(
        stdout,
        term_terminal::Clear(TermClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    let (mut cols, mut rows) = term_terminal::size().context("query terminal size")?;
    let mut grid_cols = cols.max(1) as usize;
    let mut grid_rows = rows.max(1) as usize;
    let command = terminal_command();
    let mut terminal = TerminalSession::spawn(&command, grid_cols, grid_rows)?;
    let mut should_exit = false;
    while !should_exit {
        while term_event::poll(Duration::from_millis(TUI_EVENT_POLL_MS))? {
            match term_event::read()? {
                TermEvent::Key(key) => {
                    if handle_tui_key_event(key, &mut terminal)? {
                        should_exit = true;
                        break;
                    }
                }
                TermEvent::Resize(new_cols, new_rows) => {
                    cols = new_cols;
                    rows = new_rows;
                    grid_cols = cols.max(1) as usize;
                    grid_rows = rows.max(1) as usize;
                    terminal.resize(grid_cols, grid_rows)?;
                    queue!(stdout, term_terminal::Clear(TermClearType::All))?;
                    stdout.flush().ok();
                }
                TermEvent::Paste(data) => {
                    let _ = terminal.write(data.as_bytes());
                }
                TermEvent::FocusLost | TermEvent::FocusGained => {}
                TermEvent::Mouse(_) => {}
            }
        }
        if should_exit {
            break;
        }
        terminal
            .process_output()
            .context("processing PTY output in TUI mode")?;
        let snapshot = terminal.snapshot();
        render_terminal_snapshot(&mut stdout, &snapshot)?;
        if !terminal.is_alive() {
            break;
        }
        thread::sleep(Duration::from_millis(TUI_FRAME_SLEEP_MS));
    }
    Ok(())
}

fn run_rgb_tui_cpu(demo: DemoKind, mut source: Box<dyn RgbSource>) -> Result<()> {
    let mut stdout = io::stdout();
    let _guard = TuiGuard::enter(&mut stdout)?;
    execute!(
        stdout,
        term_terminal::Clear(TermClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    let mut last_frame: Option<RgbFrame> = None;
    let mut last_tick = Instant::now();
    let mut input = TuiInputState::new(demo);
    let _audio_player = if matches!(demo, DemoKind::Doom) {
        AudioPlayer::start(false)
    } else {
        None
    };
    loop {
        while term_event::poll(Duration::from_millis(TUI_EVENT_POLL_MS))? {
            match term_event::read()? {
                TermEvent::Key(key) => {
                    if input.handle_key(&key) {
                        continue;
                    }
                    if rgb_demo_should_exit(key) {
                        return Ok(());
                    }
                }
                TermEvent::Resize(_, _) => {
                    queue!(stdout, term_terminal::Clear(TermClearType::All))?;
                }
                TermEvent::Paste(_) | TermEvent::FocusLost | TermEvent::FocusGained => {}
                TermEvent::Mouse(_) => {}
            }
        }
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        if let Some(frame) = source.update(dt)? {
            last_frame = Some(frame);
        }
        if let Some(frame) = last_frame.as_ref() {
            render_rgb_frame_ansi(&mut stdout, frame)?;
        }
        thread::sleep(Duration::from_millis(TUI_FRAME_SLEEP_MS));
    }
}

fn run_rgb_tui_gpu(demo: DemoKind) -> Result<()> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .context("request adapter for GPU TUI")?;
    let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("gpu-tui-device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
        },
        None,
    ))?;
    let mut gpu_demo = create_gpu_demo(demo, &device, &queue)?;
    let mut stdout = io::stdout();
    let _guard = TuiGuard::enter(&mut stdout)?;
    execute!(
        stdout,
        term_terminal::Clear(TermClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    let mut last_tick = Instant::now();
    let mut input = TuiInputState::new(demo);
    let _audio_player = if matches!(demo, DemoKind::Doom) {
        AudioPlayer::start(false)
    } else {
        None
    };
    loop {
        while term_event::poll(Duration::from_millis(TUI_EVENT_POLL_MS))? {
            match term_event::read()? {
                TermEvent::Key(key) => {
                    if input.handle_key(&key) {
                        continue;
                    }
                    if rgb_demo_should_exit(key) {
                        return Ok(());
                    }
                }
                TermEvent::Resize(_, _) => {
                    queue!(stdout, term_terminal::Clear(TermClearType::All))?;
                }
                TermEvent::Paste(_) | TermEvent::FocusLost | TermEvent::FocusGained => {}
                TermEvent::Mouse(_) => {}
            }
        }
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        gpu_demo.update(dt, &device, &queue)?;
        let frame = gpu_demo.capture_frame(&device, &queue)?;
        render_rgb_frame_ansi(&mut stdout, &frame)?;
        thread::sleep(Duration::from_millis(TUI_FRAME_SLEEP_MS));
    }
}

fn cpu_source_for(demo: DemoKind, opts: &RendererOptions) -> Result<Box<dyn RgbSource>> {
    match demo {
        DemoKind::Plasma => Ok(Box::new(PlasmaSource::new(RGB_DEMO_WIDTH, RGB_DEMO_HEIGHT))),
        DemoKind::Ray => Ok(Box::new(RayDemoSource::new(
            RGB_DEMO_WIDTH,
            RGB_DEMO_HEIGHT,
        ))),
        DemoKind::Doom => Ok(Box::new(DoomFeedSource::new()?)),
        DemoKind::Youtube => Ok(Box::new(YoutubeSource::new(YoutubeSettings::from_env(
            opts.youtube_audio,
        ))?)),
        DemoKind::Terminal => bail!("terminal demo does not support RGB output"),
    }
}

struct TuiInputState {
    demo: DemoKind,
    focus: InputFocus,
    writer: Option<InputFeedWriter>,
}

impl TuiInputState {
    fn new(demo: DemoKind) -> Self {
        let writer = if matches!(demo, DemoKind::Doom) {
            open_input_feed_writer()
        } else {
            None
        };
        let focus = if matches!(demo, DemoKind::Doom) && writer.is_some() {
            InputFocus::Feed
        } else {
            InputFocus::Terminal
        };
        Self {
            demo,
            focus,
            writer,
        }
    }

    fn handle_key(&mut self, key: &TermKeyEvent) -> bool {
        if is_term_focus_toggle(key) {
            return self.toggle();
        }
        if self.focus != InputFocus::Feed {
            return false;
        }
        self.dispatch_feed_key(key)
    }

    fn toggle(&mut self) -> bool {
        if self.writer.is_none() {
            return false;
        }
        self.focus = match self.focus {
            InputFocus::Terminal => InputFocus::Feed,
            InputFocus::Feed => InputFocus::Terminal,
        };
        info!("keyboard focus switched to {:?}", self.focus);
        true
    }

    fn dispatch_feed_key(&mut self, key: &TermKeyEvent) -> bool {
        let Some(writer) = self.writer.as_mut() else {
            return false;
        };
        if self.demo == DemoKind::Doom {
            if let Some(code) = doom_scancode_from_term_key(key) {
                let pressed = !matches!(key.kind, TermKeyEventKind::Release);
                let flag = if pressed { 1 } else { 0 };
                if let Err(err) = writer.write_event(&[flag, code]) {
                    warn!("input feed write failed: {err:?}");
                }
                return true;
            }
        }
        if matches!(key.kind, TermKeyEventKind::Press | TermKeyEventKind::Repeat) {
            if let Some(bytes) = term_key_to_bytes(key.clone()) {
                if let Err(err) = writer.write_event(&bytes) {
                    warn!("input feed write failed: {err:?}");
                }
                return true;
            }
        }
        false
    }
}

fn handle_tui_key_event(event: TermKeyEvent, terminal: &mut TerminalSession) -> Result<bool> {
    if !matches!(
        event.kind,
        TermKeyEventKind::Press | TermKeyEventKind::Repeat
    ) {
        return Ok(false);
    }
    if event.modifiers.contains(TermKeyModifiers::CONTROL)
        && matches!(event.code, TermKeyCode::Char('q') | TermKeyCode::Char('Q'))
    {
        return Ok(true);
    }
    if let Some(bytes) = encode_term_key_event(event) {
        let _ = terminal.write(&bytes);
    }
    Ok(false)
}

fn is_term_focus_toggle(key: &TermKeyEvent) -> bool {
    matches!(key.code, TermKeyCode::F(n) if n == 9)
}

fn doom_scancode_from_term_key(key: &TermKeyEvent) -> Option<u8> {
    let symbol = match key.code {
        TermKeyCode::Char(c) => KeySymbol::Char(c),
        TermKeyCode::Up => KeySymbol::ArrowUp,
        TermKeyCode::Down => KeySymbol::ArrowDown,
        TermKeyCode::Left => KeySymbol::ArrowLeft,
        TermKeyCode::Right => KeySymbol::ArrowRight,
        TermKeyCode::Esc => KeySymbol::Escape,
        TermKeyCode::Enter => KeySymbol::Enter,
        TermKeyCode::Tab => KeySymbol::Tab,
        TermKeyCode::Backspace => KeySymbol::Backspace,
        TermKeyCode::Home => KeySymbol::Home,
        TermKeyCode::End => KeySymbol::End,
        TermKeyCode::PageUp => KeySymbol::PageUp,
        TermKeyCode::PageDown => KeySymbol::PageDown,
        _ => return None,
    };
    doom_input::scancode_from_symbol(symbol)
}

fn term_key_to_bytes(key: TermKeyEvent) -> Option<Vec<u8>> {
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

fn rgb_demo_should_exit(key: TermKeyEvent) -> bool {
    use TermKeyCode::*;
    if key.modifiers.is_empty() && matches!(key.code, Char('q') | Char('Q') | Esc) {
        return true;
    }
    if key.modifiers.contains(TermKeyModifiers::CONTROL)
        && matches!(key.code, Char('c') | Char('C'))
    {
        return true;
    }
    false
}

fn encode_term_key_event(event: TermKeyEvent) -> Option<Vec<u8>> {
    use TermKeyCode::*;
    match event.code {
        Char(ch) => Some(encode_term_char(ch, event.modifiers)),
        Enter => Some(b"\r".to_vec()),
        Backspace => Some(vec![0x7f]),
        Tab => Some(vec![b'\t']),
        BackTab => Some(b"\x1b[Z".to_vec()),
        Esc => Some(vec![0x1b]),
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
        F(n) => f_key_sequence(n),
        Null => None,
        _ => None,
    }
}

fn encode_term_char(ch: char, modifiers: TermKeyModifiers) -> Vec<u8> {
    let mut bytes = Vec::new();
    if modifiers.contains(TermKeyModifiers::ALT) {
        bytes.push(0x1b);
    }
    if modifiers.contains(TermKeyModifiers::CONTROL) && ch.is_ascii() {
        bytes.push((ch.to_ascii_lowercase() as u8) & 0x1f);
    } else {
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        bytes.extend_from_slice(encoded.as_bytes());
    }
    bytes
}

fn f_key_sequence(index: u8) -> Option<Vec<u8>> {
    let seq = match index {
        1 => b"\x1bOP".to_vec(),
        2 => b"\x1bOQ".to_vec(),
        3 => b"\x1bOR".to_vec(),
        4 => b"\x1bOS".to_vec(),
        5 => b"\x1b[15~".to_vec(),
        6 => b"\x1b[17~".to_vec(),
        7 => b"\x1b[18~".to_vec(),
        8 => b"\x1b[19~".to_vec(),
        9 => b"\x1b[20~".to_vec(),
        10 => b"\x1b[21~".to_vec(),
        11 => b"\x1b[23~".to_vec(),
        12 => b"\x1b[24~".to_vec(),
        _ => return None,
    };
    Some(seq)
}

fn render_rgb_frame_ansi(stdout: &mut io::Stdout, frame: &RgbFrame) -> Result<()> {
    let (cols, rows) = term_terminal::size().context("query terminal size")?;
    if cols == 0 || rows == 0 {
        return Ok(());
    }
    if let Some(image) = rgb_frame_to_image(frame) {
        let dyn_img = DynamicImage::ImageRgb8(image);
        let cols_f = cols.max(1) as f32;
        let rows_f = rows.max(1) as f32;
        let img_aspect = frame.height().max(1) as f32 / frame.width().max(1) as f32;
        let mut width_chars = cols_f;
        let mut height_chars = img_aspect * DEFAULT_CELL_ASPECT * width_chars;
        if height_chars > rows_f {
            width_chars = (rows_f / (img_aspect * DEFAULT_CELL_ASPECT)).max(1.0);
            height_chars = img_aspect * DEFAULT_CELL_ASPECT * width_chars;
        }
        let width_chars = width_chars.floor().max(1.0).min(cols_f) as u32;
        let height_chars = height_chars.round().max(1.0).min(rows_f) as u32;
        let art = convert_image_to_blocks(
            &dyn_img,
            width_chars,
            Some(height_chars),
            DEFAULT_CELL_ASPECT,
        )?;
        queue!(
            stdout,
            cursor::MoveTo(0, 0),
            term_terminal::Clear(TermClearType::All)
        )?;
        let offset_x = ((cols as i32 - width_chars as i32) / 2).max(0) as u16;
        let offset_y = ((rows as i32 - height_chars as i32) / 2).max(0) as u16;
        for (row_idx, line) in art.lines().enumerate() {
            let row = offset_y + row_idx as u16;
            queue!(stdout, cursor::MoveTo(offset_x, row))?;
            stdout.write_all(line.as_bytes())?;
            queue!(stdout, term_terminal::Clear(TermClearType::UntilNewLine))?;
        }
        stdout.flush()?;
    }
    Ok(())
}

fn rgb_frame_to_image(frame: &RgbFrame) -> Option<RgbImage> {
    RgbImage::from_vec(frame.width(), frame.height(), frame.pixels().to_vec())
}

fn render_terminal_snapshot(
    stdout: &mut io::Stdout,
    snapshot: &TerminalBufferSnapshot,
) -> Result<()> {
    let mut current_fg: Option<(u8, u8, u8)> = None;
    let mut current_bg: Option<(u8, u8, u8)> = None;
    for row in 0..snapshot.rows {
        queue!(stdout, cursor::MoveTo(0, row as u16))?;
        for col in 0..snapshot.cols {
            let Some(cell) = snapshot.cell(row, col) else {
                continue;
            };
            let mut fg = cell.fg;
            let mut bg = cell.bg;
            if row == snapshot.cursor_row && col == snapshot.cursor_col {
                std::mem::swap(&mut fg, &mut bg);
            }
            apply_cell_colors(stdout, &mut current_fg, &mut current_bg, fg, bg)?;
            write_cell_char(stdout, cell.ch)?;
        }
    }
    queue!(stdout, ResetColor)?;
    stdout.flush()?;
    Ok(())
}

fn apply_cell_colors(
    stdout: &mut io::Stdout,
    current_fg: &mut Option<(u8, u8, u8)>,
    current_bg: &mut Option<(u8, u8, u8)>,
    fg: TermRgb,
    bg: TermRgb,
) -> Result<()> {
    let fg_tuple = (fg.r, fg.g, fg.b);
    if current_fg.as_ref() != Some(&fg_tuple) {
        queue!(stdout, SetForegroundColor(rgb_to_term_color(fg)))?;
        *current_fg = Some(fg_tuple);
    }
    let bg_tuple = (bg.r, bg.g, bg.b);
    if current_bg.as_ref() != Some(&bg_tuple) {
        queue!(stdout, SetBackgroundColor(rgb_to_term_color(bg)))?;
        *current_bg = Some(bg_tuple);
    }
    Ok(())
}

fn write_cell_char(stdout: &mut io::Stdout, ch: char) -> Result<()> {
    let mut buf = [0u8; 4];
    let encoded = ch.encode_utf8(&mut buf);
    stdout
        .write_all(encoded.as_bytes())
        .context("writing cell glyph to stdout")?;
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3];
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    inner: GlyphVertex,
}

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    text_pipeline: TextPipeline,
    layout_engine: LayoutEngine,
    backend: GuiBackend,
    terminal: TerminalSession,
    input_focus: InputFocus,
    input_feed: Option<InputFeedWriter>,
    demo: DemoKind,
    _audio_player: Option<AudioPlayer>,
    fps_logger: Option<FpsLogger>,
    last_frame_time: Instant,
}

enum GuiBackend {
    Terminal,
    Cpu(CpuDemoState),
    Gpu(GpuDemoState),
}

enum BackendPayload<'a> {
    Glyphs(Vec<GlyphVertex>),
    Frame {
        renderer: &'a FrameRenderer,
        vertices: Vec<FrameVertex>,
    },
}

impl State {
    async fn new(
        window: Arc<winit::window::Window>,
        demo: DemoKind,
        compute: ComputeMode,
        opts: RendererOptions,
    ) -> Result<Self> {
        let layout = load_layout()?;
        let mut layout_engine = LayoutEngine::new(&layout)?;
        let size = window.inner_size();
        let initial_rects = layout_engine.compute_layout(size.width as f32, size.height as f32)?;
        let terminal_rect = find_terminal_rect(&initial_rects)
            .unwrap_or(default_terminal_rect(size.width as f32, size.height as f32));
        let (terminal_cols, terminal_rows) = terminal_grid_size(terminal_rect);

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("request adapter")?;
        let features = wgpu::Features::POLYGON_MODE_LINE;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("render-device"),
                    required_features: features,
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("grid.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid-pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Line,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let text_pipeline = TextPipeline::new(&device, &queue, config.format);
        let command = terminal_command();
        let terminal = TerminalSession::spawn(&command, terminal_cols, terminal_rows)?;
        let backend = match demo {
            DemoKind::Terminal => GuiBackend::Terminal,
            _ => match compute {
                ComputeMode::Cpu => GuiBackend::Cpu(CpuDemoState::new(
                    cpu_source_for(demo, &opts)?,
                    &device,
                    config.format,
                )),
                ComputeMode::Gpu => GuiBackend::Gpu(GpuDemoState::new(
                    create_gpu_demo(demo, &device, &queue)?,
                    &device,
                    config.format,
                )),
            },
        };
        let input_feed = if matches!(demo, DemoKind::Doom) {
            open_input_feed_writer()
        } else {
            None
        };
        let input_focus = if matches!(demo, DemoKind::Doom) && input_feed.is_some() {
            InputFocus::Feed
        } else {
            InputFocus::Terminal
        };
        let audio_player = if matches!(demo, DemoKind::Doom) {
            AudioPlayer::start(true)
        } else {
            None
        };
        let fps_logger = opts
            .fps_sample_interval
            .map(|interval| FpsLogger::new("gui", interval));

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            text_pipeline,
            layout_engine,
            backend,
            terminal,
            input_focus,
            input_feed,
            demo,
            _audio_player: audio_player,
            fps_logger,
            last_frame_time: Instant::now(),
        })
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn handle_key(
        &mut self,
        event: KeyEvent,
        target: &winit::event_loop::EventLoopWindowTarget<()>,
    ) {
        if is_focus_toggle(&event) && self.toggle_input_focus() {
            return;
        }
        match self.input_focus {
            InputFocus::Terminal => {
                if event.state == ElementState::Pressed {
                    if let Some(text) = event.text.as_ref() {
                        if !text.is_empty() {
                            let _ = self.terminal.write(text.as_bytes());
                        }
                    } else if let Some(seq) = special_key_sequence(&event) {
                        let _ = self.terminal.write(&seq);
                    }
                }
            }
            InputFocus::Feed => {
                if self.dispatch_feed_event(&event) {
                    return;
                }
            }
        }
        if event.state == ElementState::Pressed
            && matches!(
                event.logical_key,
                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
            )
        {
            target.exit();
        }
    }

    fn dispatch_feed_event(&mut self, event: &KeyEvent) -> bool {
        let Some(writer) = self.input_feed.as_mut() else {
            return false;
        };
        if self.demo == DemoKind::Doom {
            if let Some(code) = doom_scancode_from_winit(event) {
                let flag = if matches!(event.state, ElementState::Pressed) {
                    1
                } else {
                    0
                };
                if let Err(err) = writer.write_event(&[flag, code]) {
                    warn!("input feed write failed: {err:?}");
                }
                return true;
            }
        }
        if matches!(event.state, ElementState::Pressed) {
            if let Some(text) = event.text.as_ref() {
                if !text.is_empty() {
                    if let Err(err) = writer.write_event(text.as_bytes()) {
                        warn!("input feed write failed: {err:?}");
                    }
                    return true;
                }
            } else if let Some(seq) = special_key_sequence(event) {
                if let Err(err) = writer.write_event(&seq) {
                    warn!("input feed write failed: {err:?}");
                }
                return true;
            }
        }
        false
    }

    fn toggle_input_focus(&mut self) -> bool {
        if self.input_feed.is_none() {
            return false;
        }
        self.input_focus = match self.input_focus {
            InputFocus::Terminal => InputFocus::Feed,
            InputFocus::Feed => InputFocus::Terminal,
        };
        info!("keyboard focus switched to {:?}", self.input_focus);
        true
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        let window_width = self.config.width.max(1) as f32;
        let window_height = self.config.height.max(1) as f32;
        let rects = match self
            .layout_engine
            .compute_layout(window_width, window_height)
        {
            Ok(rects) => rects,
            Err(err) => {
                info!("layout compute error: {err:?}");
                return Ok(());
            }
        };
        let vertices = build_layout_vertices(&rects, window_width, window_height);
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("layout-vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let pane_rect = find_terminal_rect(&rects)
            .unwrap_or(default_terminal_rect(window_width, window_height));
        let payload = match &mut self.backend {
            GuiBackend::Terminal => {
                if let Err(err) = self.terminal.process_output() {
                    info!("terminal read error: {err:?}");
                }
                let snapshot = self.terminal.snapshot();
                let glyphs = build_terminal_vertices(
                    &snapshot,
                    pane_rect,
                    window_width,
                    window_height,
                    self.text_pipeline.atlas(),
                );
                BackendPayload::Glyphs(glyphs)
            }
            GuiBackend::Cpu(demo) => {
                if let Err(err) = demo.update(dt, &self.device, &self.queue) {
                    info!("cpu demo update error: {err:?}");
                }
                let verts = build_frame_vertices(pane_rect, window_width, window_height);
                BackendPayload::Frame {
                    renderer: demo.renderer(),
                    vertices: verts,
                }
            }
            GuiBackend::Gpu(demo) => {
                if let Err(err) = demo.update(dt, &self.device, &self.queue) {
                    info!("gpu demo update error: {err:?}");
                }
                let verts = build_frame_vertices(pane_rect, window_width, window_height);
                BackendPayload::Frame {
                    renderer: demo.renderer(),
                    vertices: verts,
                }
            }
        };
        let mut glyph_draw: Option<(wgpu::Buffer, u32)> = None;
        let mut frame_draw: Option<(wgpu::Buffer, u32, &wgpu::RenderPipeline, &wgpu::BindGroup)> =
            None;
        match payload {
            BackendPayload::Glyphs(glyphs) => {
                if !glyphs.is_empty() {
                    let glyph_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("glyph-vertices"),
                                contents: bytemuck::cast_slice(&glyphs),
                                usage: wgpu::BufferUsages::VERTEX,
                            });
                    glyph_draw = Some((glyph_buffer, glyphs.len() as u32));
                }
            }
            BackendPayload::Frame { renderer, vertices } => {
                if !vertices.is_empty() {
                    if let Some(bind_group) = renderer.bind_group() {
                        let frame_buffer =
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("frame-vertices"),
                                    contents: bytemuck::cast_slice(&vertices),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        frame_draw = Some((
                            frame_buffer,
                            vertices.len() as u32,
                            renderer.pipeline(),
                            bind_group,
                        ));
                    }
                }
            }
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.09,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);

            if let Some((buffer, count)) = glyph_draw.as_ref() {
                pass.set_pipeline(self.text_pipeline.pipeline());
                pass.set_bind_group(0, self.text_pipeline.bind_group(), &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            } else if let Some((buffer, count, pipeline, bind_group)) = frame_draw.as_ref() {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, *bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        if let Some(logger) = self.fps_logger.as_mut() {
            logger.tick();
        }
        Ok(())
    }
}

fn build_layout_vertices(rects: &[(ComponentKind, Rect)], width: f32, height: f32) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    for (component, rect) in rects {
        let color = match component {
            ComponentKind::MenuBar => [0.3, 0.3, 0.35],
            ComponentKind::OverlayRegion => [0.25, 0.65, 0.25],
            ComponentKind::TerminalPane => [0.15, 0.35, 0.65],
            ComponentKind::Unknown => [0.2, 0.2, 0.2],
        };
        push_rect(&mut vertices, *rect, color, width, height);
    }
    vertices
}

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn find_terminal_rect(rects: &[(ComponentKind, Rect)]) -> Option<Rect> {
    rects.iter().find_map(|(component, rect)| {
        if matches!(component, ComponentKind::TerminalPane) {
            Some(*rect)
        } else {
            None
        }
    })
}

fn default_terminal_rect(width: f32, height: f32) -> Rect {
    let usable_height = (height - MENU_BAR_HEIGHT - OVERLAY_HEIGHT).max(0.0);
    Rect {
        x: 0.0,
        y: MENU_BAR_HEIGHT,
        width,
        height: usable_height,
    }
}

fn terminal_grid_size(rect: Rect) -> (usize, usize) {
    let cols = (rect.width / CELL_PIXEL_WIDTH).floor().max(1.0) as usize;
    let rows = (rect.height / CELL_PIXEL_HEIGHT).floor().max(1.0) as usize;
    (cols.max(1), rows.max(1))
}

fn terminal_command() -> String {
    if let Ok(custom) = std::env::var("TUI_GPU_TERMINAL_COMMAND") {
        return custom;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    format!("printf \"tui-gpu ready\\n\"; exec {}", shell)
}

fn build_terminal_vertices(
    snapshot: &TerminalBufferSnapshot,
    pane_rect: Rect,
    window_width: f32,
    window_height: f32,
    atlas: &text::GlyphAtlas,
) -> Vec<GlyphVertex> {
    if pane_rect.width <= 0.0 || pane_rect.height <= 0.0 {
        return Vec::new();
    }
    let mut vertices = Vec::with_capacity(snapshot.rows * snapshot.cols * 6);
    let pane_right = pane_rect.x + pane_rect.width;
    let pane_bottom = pane_rect.y + pane_rect.height;
    for row in 0..snapshot.rows {
        let y = pane_rect.y + row as f32 * CELL_PIXEL_HEIGHT;
        if y >= pane_bottom {
            break;
        }
        let max_y = y + CELL_PIXEL_HEIGHT;
        if max_y > pane_bottom {
            break;
        }
        for col in 0..snapshot.cols {
            let x = pane_rect.x + col as f32 * CELL_PIXEL_WIDTH;
            if x >= pane_right {
                break;
            }
            let max_x = x + CELL_PIXEL_WIDTH;
            if max_x > pane_right {
                break;
            }
            if let Some(cell) = snapshot.cell(row, col) {
                if cell.ch == ' ' {
                    continue;
                }
                let rect = Rect {
                    x,
                    y,
                    width: CELL_PIXEL_WIDTH,
                    height: CELL_PIXEL_HEIGHT,
                };
                push_glyph_quad(
                    &mut vertices,
                    rect,
                    atlas.glyph_uv(cell.ch),
                    rgb_to_vec3(cell.fg),
                    window_width,
                    window_height,
                );
            }
        }
    }
    vertices
}

fn build_frame_vertices(rect: Rect, window_width: f32, window_height: f32) -> Vec<FrameVertex> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Vec::new();
    }
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.width;
    let y1 = rect.y + rect.height;
    let top_left = FrameVertex {
        position: px_to_ndc(x0, y0, window_width, window_height),
        uv: [0.0, 0.0],
    };
    let top_right = FrameVertex {
        position: px_to_ndc(x1, y0, window_width, window_height),
        uv: [1.0, 0.0],
    };
    let bottom_right = FrameVertex {
        position: px_to_ndc(x1, y1, window_width, window_height),
        uv: [1.0, 1.0],
    };
    let bottom_left = FrameVertex {
        position: px_to_ndc(x0, y1, window_width, window_height),
        uv: [0.0, 1.0],
    };
    vec![
        top_left,
        top_right,
        bottom_right,
        top_left,
        bottom_right,
        bottom_left,
    ]
}

fn push_rect(vertices: &mut Vec<Vertex>, rect: Rect, color: [f32; 3], width: f32, height: f32) {
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.width;
    let y1 = rect.y + rect.height;
    let v0 = Vertex {
        position: px_to_ndc(x0, y0, width, height),
        color,
    };
    let v1 = Vertex {
        position: px_to_ndc(x1, y0, width, height),
        color,
    };
    let v2 = Vertex {
        position: px_to_ndc(x1, y1, width, height),
        color,
    };
    let v3 = Vertex {
        position: px_to_ndc(x0, y1, width, height),
        color,
    };
    vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}

fn px_to_ndc(px: f32, py: f32, width: f32, height: f32) -> [f32; 2] {
    let x = (px / width) * 2.0 - 1.0;
    let y = 1.0 - (py / height) * 2.0;
    [x, y]
}

fn special_key_sequence(event: &KeyEvent) -> Option<Vec<u8>> {
    use winit::keyboard::{KeyCode, PhysicalKey};
    if let PhysicalKey::Code(code) = &event.physical_key {
        match code {
            KeyCode::Backspace => Some(vec![0x7f]),
            KeyCode::Enter => Some(b"\r".to_vec()),
            KeyCode::Tab => Some(vec![b'\t']),
            KeyCode::Escape => Some(vec![0x1b]),
            KeyCode::ArrowUp => Some(b"\x1b[A".to_vec()),
            KeyCode::ArrowDown => Some(b"\x1b[B".to_vec()),
            KeyCode::ArrowRight => Some(b"\x1b[C".to_vec()),
            KeyCode::ArrowLeft => Some(b"\x1b[D".to_vec()),
            KeyCode::Home => Some(b"\x1b[H".to_vec()),
            KeyCode::End => Some(b"\x1b[F".to_vec()),
            KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
            KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
            KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
            KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
            _ => None,
        }
    } else {
        None
    }
}

fn load_layout() -> Result<LayoutDocument> {
    let content = std::fs::read_to_string(LAYOUT_PATH)
        .with_context(|| format!("failed to read layout {LAYOUT_PATH}"))?;
    let doc: LayoutDocument = serde_json::from_str(&content)?;
    Ok(doc)
}

struct LayoutEngine {
    taffy: Taffy,
    root: Node,
    nodes: Vec<NodeInfo>,
}

struct NodeInfo {
    component: ComponentKind,
    node: Node,
}

impl LayoutEngine {
    fn new(doc: &LayoutDocument) -> Result<Self> {
        let mut taffy = Taffy::new();
        let mut nodes = Vec::new();
        for node in &doc.nodes {
            let style = style_from_layout_node(node);
            let handle = taffy.new_leaf(style)?;
            nodes.push(NodeInfo {
                component: node.component,
                node: handle,
            });
        }
        let children: Vec<Node> = nodes.iter().map(|n| n.node).collect();
        let root_style = Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
            },
            flex_direction: TaffyFlexDirection::Column,
            ..Default::default()
        };
        let root = taffy.new_with_children(root_style, &children)?;
        Ok(Self { taffy, root, nodes })
    }

    fn compute_layout(&mut self, width: f32, height: f32) -> Result<Vec<(ComponentKind, Rect)>> {
        self.taffy.compute_layout(
            self.root,
            Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::Definite(height),
            },
        )?;
        let mut rects = Vec::new();
        for info in &self.nodes {
            let layout = self.taffy.layout(info.node)?;
            rects.push((
                info.component,
                Rect {
                    x: layout.location.x,
                    y: layout.location.y,
                    width: layout.size.width,
                    height: layout.size.height,
                },
            ));
        }
        Ok(rects)
    }
}

fn style_from_layout_node(node: &LayoutNode) -> Style {
    match node.component {
        ComponentKind::MenuBar => Style {
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Points(MENU_BAR_HEIGHT),
            },
            ..Default::default()
        },
        ComponentKind::OverlayRegion => Style {
            position: Position::Absolute,
            inset: TaffyRect {
                left: LengthPercentageAuto::Auto,
                right: LengthPercentageAuto::Auto,
                top: LengthPercentageAuto::Auto,
                bottom: LengthPercentageAuto::Points(0.0),
            },
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Points(OVERLAY_HEIGHT),
            },
            ..Default::default()
        },
        ComponentKind::TerminalPane => Style {
            flex_grow: node.layout.flex_grow,
            size: Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Auto,
            },
            ..Default::default()
        },
        ComponentKind::Unknown => Style::DEFAULT,
    }
}

fn push_glyph_quad(
    vertices: &mut Vec<GlyphVertex>,
    rect: Rect,
    uv: text::GlyphUv,
    color: [f32; 3],
    width: f32,
    height: f32,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.width;
    let y1 = rect.y + rect.height;
    let v0 = GlyphVertex {
        position: px_to_ndc(x0, y0, width, height),
        uv: [uv.u0, uv.v0],
        color,
    };
    let v1 = GlyphVertex {
        position: px_to_ndc(x1, y0, width, height),
        uv: [uv.u1, uv.v0],
        color,
    };
    let v2 = GlyphVertex {
        position: px_to_ndc(x1, y1, width, height),
        uv: [uv.u1, uv.v1],
        color,
    };
    let v3 = GlyphVertex {
        position: px_to_ndc(x0, y1, width, height),
        uv: [uv.u0, uv.v1],
        color,
    };
    vertices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
}

fn rgb_to_vec3(color: TermRgb) -> [f32; 3] {
    [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
    ]
}

fn rgb_to_term_color(color: TermRgb) -> TermColor {
    TermColor::Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

fn open_input_feed_writer() -> Option<InputFeedWriter> {
    let path = env::var("TUI_GPU_INPUT_FEED").unwrap_or_else(|_| DEFAULT_INPUT_PATH.to_string());
    match InputFeedWriter::open(&path, DEFAULT_INPUT_CAPACITY) {
        Ok(writer) => Some(writer),
        Err(err) => {
            warn!("input feed unavailable at {path}: {err:?}");
            None
        }
    }
}

fn is_focus_toggle(event: &KeyEvent) -> bool {
    matches!(event.logical_key, Key::Named(NamedKey::F9))
}

fn doom_scancode_from_winit(event: &KeyEvent) -> Option<u8> {
    let symbol = match &event.logical_key {
        Key::Character(text) => text.chars().next().map(KeySymbol::Char)?,
        Key::Named(NamedKey::ArrowUp) => KeySymbol::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => KeySymbol::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => KeySymbol::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => KeySymbol::ArrowRight,
        Key::Named(NamedKey::Escape) => KeySymbol::Escape,
        Key::Named(NamedKey::Enter) => KeySymbol::Enter,
        Key::Named(NamedKey::Tab) => KeySymbol::Tab,
        Key::Named(NamedKey::Backspace) => KeySymbol::Backspace,
        Key::Named(NamedKey::Home) => KeySymbol::Home,
        Key::Named(NamedKey::End) => KeySymbol::End,
        Key::Named(NamedKey::PageUp) => KeySymbol::PageUp,
        Key::Named(NamedKey::PageDown) => KeySymbol::PageDown,
        Key::Named(NamedKey::Space) => KeySymbol::Space,
        _ => return None,
    };
    doom_input::scancode_from_symbol(symbol)
}

struct CpuDemoState {
    source: Box<dyn RgbSource>,
    renderer: FrameRenderer,
}

impl CpuDemoState {
    fn new(source: Box<dyn RgbSource>, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            source,
            renderer: FrameRenderer::new(device, format),
        }
    }

    fn update(&mut self, dt: f32, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
        if let Some(frame) = self.source.update(dt)? {
            self.renderer.upload_frame(device, queue, &frame)?;
        }
        Ok(())
    }

    fn renderer(&self) -> &FrameRenderer {
        &self.renderer
    }
}

struct AudioPlayer {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    _stream: OutputStream,
}

impl AudioPlayer {
    fn start(wait_for_writer: bool) -> Option<Self> {
        let reader = match open_audio_reader(wait_for_writer) {
            Some(reader) => reader,
            None => return None,
        };
        let (stream, handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&handle).ok()?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || run_audio_loop(reader, sink, thread_stop));
        Some(Self {
            stop,
            thread: Some(thread),
            _stream: stream,
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

fn open_audio_reader(wait_for_writer: bool) -> Option<AudioFeedReader> {
    let path = env::var("TUI_GPU_AUDIO_FEED").unwrap_or_else(|_| DEFAULT_AUDIO_PATH.to_string());
    let mut attempts = 0;
    loop {
        match AudioFeedReader::open(&path, DEFAULT_AUDIO_CAPACITY) {
            Ok(reader) => return Some(reader),
            Err(err) => {
                if !wait_for_writer || attempts > 50 {
                    warn!("audio feed unavailable at {path}: {err:?}");
                    return None;
                }
                attempts += 1;
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn run_audio_loop(mut reader: AudioFeedReader, sink: Sink, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match reader.poll_event() {
            Ok(Some(event)) => {
                if let Some(buffer) = audio_buffer_from_event(&event) {
                    sink.append(buffer);
                }
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(err) => {
                warn!("audio feed read error: {err:?}");
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

struct FpsLogger {
    label: &'static str,
    interval: Duration,
    last_report: Instant,
    frames: u32,
}

impl FpsLogger {
    fn new(label: &'static str, interval: Duration) -> Self {
        Self {
            label,
            interval,
            last_report: Instant::now(),
            frames: 0,
        }
    }

    fn tick(&mut self) {
        self.frames += 1;
        let elapsed = self.last_report.elapsed();
        if elapsed >= self.interval {
            let fps = self.frames as f32 / elapsed.as_secs_f32().max(0.0001);
            info!("fps[{}] {:.1}", self.label, fps);
            self.frames = 0;
            self.last_report = Instant::now();
        }
    }
}

struct GpuDemoState {
    demo: Box<dyn GpuDemo>,
    renderer: FrameRenderer,
    bound_texture: bool,
}

impl GpuDemoState {
    fn new(demo: Box<dyn GpuDemo>, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            demo,
            renderer: FrameRenderer::new(device, format),
            bound_texture: false,
        }
    }

    fn update(&mut self, dt: f32, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
        self.demo.update(dt, device, queue)?;
        if !self.bound_texture {
            self.renderer
                .set_external_texture(device, self.demo.texture_view());
            self.bound_texture = true;
        }
        Ok(())
    }

    fn renderer(&self) -> &FrameRenderer {
        &self.renderer
    }
}

struct TuiGuard;

impl TuiGuard {
    fn enter(stdout: &mut io::Stdout) -> Result<Self> {
        term_terminal::enable_raw_mode()?;
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
        let _ = execute!(
            stdout,
            PopKeyboardEnhancementFlags,
            ResetColor,
            cursor::Show
        );
        let _ = term_terminal::disable_raw_mode();
    }
}
