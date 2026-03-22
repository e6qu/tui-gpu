use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use ansi_image::{
    convert_image_to_ansi, convert_image_to_blocks, convert_image_to_braille, DEFAULT_CELL_ASPECT,
    DEFAULT_PALETTE, DENSE_PALETTE,
};
use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use image::DynamicImage;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};

#[derive(Parser, Debug)]
struct Cli {
    /// YouTube URL to stream (requires yt-dlp)
    #[arg(long)]
    url: Option<String>,
    /// Local video file to read instead of a URL
    #[arg(long)]
    input: Option<PathBuf>,
    /// Target ANSI width in characters
    #[arg(long, default_value_t = 80)]
    width: u32,
    /// Optional target height in rows; defaults to keeping aspect ratio
    #[arg(long)]
    height: Option<u32>,
    /// Path to the yt-dlp executable
    #[arg(long, default_value = "yt-dlp")]
    ytdlp: String,
    /// Path to the ffmpeg executable
    #[arg(long, default_value = "ffmpeg")]
    ffmpeg: String,
    /// Playback rate limiter (frames per second)
    #[arg(long, default_value_t = 24)]
    fps: u32,
    /// yt-dlp video format selector (video-only preferred)
    #[arg(long, default_value = DEFAULT_YTDLP_VIDEO_FORMAT)]
    video_format: String,
    /// yt-dlp audio format selector
    #[arg(long, default_value = DEFAULT_YTDLP_AUDIO_FORMAT)]
    audio_format: String,
    /// Disable audio playback
    #[arg(long)]
    no_audio: bool,
    /// Glyph rendering mode
    #[arg(long, value_enum, default_value_t = GlyphMode::Blocks)]
    glyph_mode: GlyphMode,
    /// Override ASCII palette used by `--glyph-mode palette`
    #[arg(long)]
    ascii_palette: Option<String>,
    /// Predefined ASCII palette when no override is supplied
    #[arg(long, value_enum, default_value_t = AsciiPalettePreset::Classic)]
    ascii_preset: AsciiPalettePreset,
}

const DEFAULT_YTDLP_VIDEO_FORMAT: &str =
    "bestvideo[ext=mp4][vcodec^=avc1]/bestvideo[ext=mp4]/bestvideo";
const DEFAULT_YTDLP_AUDIO_FORMAT: &str = "bestaudio/best";
const AUDIO_SAMPLE_RATE: u32 = 44100;

struct VideoPipe {
    _ffmpeg: Child,
}

enum VideoSource {
    Network(String),
    File(PathBuf),
}

struct Sources {
    video: VideoSource,
    audio: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum GlyphMode {
    Blocks,
    Braille,
    Palette,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AsciiPalettePreset {
    Classic,
    Dense,
}

impl AsciiPalettePreset {
    fn palette(self) -> &'static str {
        match self {
            AsciiPalettePreset::Classic => DEFAULT_PALETTE,
            AsciiPalettePreset::Dense => DENSE_PALETTE,
        }
    }
}

impl VideoPipe {
    fn spawn(cli: &Cli, source: VideoSource) -> Result<(Self, ChildStdout)> {
        let scale = match cli.height {
            Some(h) => format!("scale={}:{}", cli.width, h),
            None => format!("scale={}:{}", cli.width, -1),
        };
        let mut ffmpeg_cmd = Command::new(&cli.ffmpeg);
        ffmpeg_cmd.arg("-loglevel").arg("warning").arg("-nostdin");
        match source {
            VideoSource::Network(url) => {
                ffmpeg_cmd.arg("-i").arg(url);
                ffmpeg_cmd.stdin(Stdio::null());
            }
            VideoSource::File(path) => {
                ffmpeg_cmd.arg("-i").arg(path);
                ffmpeg_cmd.stdin(Stdio::null());
            }
        }
        ffmpeg_cmd
            .arg("-an")
            .arg("-sn")
            .arg("-vf")
            .arg(scale)
            .arg("-f")
            .arg("image2pipe")
            .arg("-vcodec")
            .arg("png")
            .arg("-")
            .stdout(Stdio::piped());
        let mut ffmpeg_child = ffmpeg_cmd
            .spawn()
            .with_context(|| format!("spawning {}", cli.ffmpeg))?;
        let stdout = ffmpeg_child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("ffmpeg stdout unavailable"))?;
        Ok((
            Self {
                _ffmpeg: ffmpeg_child,
            },
            stdout,
        ))
    }
}

impl Drop for VideoPipe {
    fn drop(&mut self) {
        match self._ffmpeg.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = self._ffmpeg.kill();
                let _ = self._ffmpeg.wait();
            }
            Err(_) => {
                let _ = self._ffmpeg.kill();
            }
        }
    }
}

struct AudioPlayer {
    child: Child,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    _stream: OutputStream,
}

impl AudioPlayer {
    fn start(ffmpeg: &str, url: &str) -> Result<Self> {
        let mut cmd = Command::new(ffmpeg);
        cmd.arg("-loglevel")
            .arg("warning")
            .arg("-nostdin")
            .arg("-i")
            .arg(url)
            .arg("-f")
            .arg("s16le")
            .arg("-ac")
            .arg("2")
            .arg("-ar")
            .arg(AUDIO_SAMPLE_RATE.to_string())
            .arg("-");
        cmd.stdout(Stdio::piped());
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning {} for audio playback", ffmpeg))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("audio ffmpeg stdout unavailable"))?;
        let (stream, handle) =
            OutputStream::try_default().context("initializing audio output stream")?;
        let sink = Sink::try_new(&handle).context("creating audio sink")?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let thread = std::thread::spawn(move || run_audio_loop(stdout, sink, stop_clone));
        Ok(Self {
            child,
            stop,
            thread: Some(thread),
            _stream: stream,
        })
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let ascii_palette = build_ascii_palette(&cli);
    let sources = resolve_sources(&cli)?;
    let _audio_player = if let Some(audio_url) = sources.audio.as_deref() {
        Some(AudioPlayer::start(&cli.ffmpeg, audio_url)?)
    } else {
        None
    };
    let (mut _pipe, mut stream) = VideoPipe::spawn(&cli, sources.video)?;
    print!("\x1b[2J");
    let sleep = Duration::from_millis((1000 / cli.fps.max(1)) as u64);
    loop {
        match read_png_frame(&mut stream)? {
            Some(frame) => {
                let img =
                    image::load_from_memory(&frame).context("decoding PNG frame from ffmpeg")?;
                render_frame(&img, cli.width, cli.height, &ascii_palette, cli.glyph_mode)?;
                thread::sleep(sleep);
            }
            None => break,
        }
    }
    Ok(())
}

fn resolve_sources(cli: &Cli) -> Result<Sources> {
    if let Some(url) = &cli.url {
        let video = fetch_stream_url(&cli.ytdlp, &cli.video_format, url)?;
        let audio = if cli.no_audio {
            None
        } else {
            Some(fetch_stream_url(&cli.ytdlp, &cli.audio_format, url)?)
        };
        Ok(Sources {
            video: VideoSource::Network(video),
            audio,
        })
    } else if let Some(path) = cli.input.as_ref() {
        Ok(Sources {
            video: VideoSource::File(path.clone()),
            audio: None,
        })
    } else {
        Err(anyhow!("provide either --url or --input"))
    }
}

fn build_ascii_palette(cli: &Cli) -> Vec<char> {
    if let Some(custom) = cli.ascii_palette.as_ref() {
        let mut chars: Vec<char> = custom.chars().collect();
        if chars.is_empty() {
            chars = DEFAULT_PALETTE.chars().collect();
        }
        chars
    } else {
        cli.ascii_preset.palette().chars().collect()
    }
}

fn fetch_stream_url(ytdlp: &str, format: &str, video_url: &str) -> Result<String> {
    let output = Command::new(ytdlp)
        .args([
            "--get-url",
            "--format",
            format,
            "--quiet",
            "--no-warnings",
            video_url,
        ])
        .output()
        .with_context(|| format!("spawning {}", ytdlp))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("yt-dlp failed: {}", stderr.trim());
    }
    let out = String::from_utf8(output.stdout)?;
    let mut lines = out.lines().filter(|line| !line.trim().is_empty());
    lines
        .next()
        .map(|line| line.trim().to_string())
        .ok_or_else(|| anyhow!("yt-dlp did not return a stream URL"))
}

fn run_audio_loop(mut stdout: ChildStdout, sink: Sink, stop: Arc<AtomicBool>) {
    use std::io::ErrorKind;
    let mut buf = vec![0u8; 8192];
    let mut pending = Vec::new();
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                let usable = pending.len() - (pending.len() % 4);
                if usable == 0 {
                    continue;
                }
                let mut samples = Vec::with_capacity(usable / 2);
                for chunk in pending[..usable].chunks_exact(2) {
                    samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
                }
                pending.drain(0..usable);
                if !samples.is_empty() {
                    let buffer = SamplesBuffer::new(2, AUDIO_SAMPLE_RATE, samples);
                    sink.append(buffer);
                }
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    sink.sleep_until_end();
}

fn render_frame(
    image: &DynamicImage,
    width: u32,
    height: Option<u32>,
    palette: &[char],
    glyph_mode: GlyphMode,
) -> Result<()> {
    let art = match glyph_mode {
        GlyphMode::Braille => convert_image_to_braille(image, width, height)?,
        GlyphMode::Palette => {
            convert_image_to_ansi(image, width, height, palette, DEFAULT_CELL_ASPECT)?
        }
        GlyphMode::Blocks => convert_image_to_blocks(image, width, height, DEFAULT_CELL_ASPECT)?,
    };
    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b[H")?;
    stdout.write_all(art.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn read_png_frame(reader: &mut dyn Read) -> Result<Option<Vec<u8>>> {
    const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    let mut sig_buf = [0u8; 8];
    if !read_exact_allow_eof(reader, &mut sig_buf)? {
        return Ok(None);
    }
    if sig_buf != SIG {
        return Err(anyhow!(
            "unexpected stream content (PNG signature mismatch)"
        ));
    }
    let mut data = Vec::new();
    data.extend_from_slice(&sig_buf);
    loop {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        data.extend_from_slice(&len_buf);
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut chunk = vec![0u8; len + 4];
        reader.read_exact(&mut chunk)?;
        data.extend_from_slice(&chunk);
        let mut crc = [0u8; 4];
        reader.read_exact(&mut crc)?;
        data.extend_from_slice(&crc);
        if &chunk[..4] == b"IEND" {
            break;
        }
    }
    Ok(Some(data))
}

fn read_exact_allow_eof(reader: &mut dyn Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut read = 0;
    while read < buf.len() {
        match reader.read(&mut buf[read..])? {
            0 => return Ok(read != 0),
            n => read += n,
        }
    }
    Ok(true)
}
