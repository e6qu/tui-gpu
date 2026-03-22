use anyhow::{anyhow, bail, Context, Result};
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::{
    env,
    io::Read,
    path::PathBuf,
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, Receiver, SyncSender, TryRecvError},
        Arc,
    },
    thread,
};

use crate::{rgb_frame::RgbFrame, RGB_DEMO_HEIGHT, RGB_DEMO_WIDTH};

const DEFAULT_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
const DEFAULT_VIDEO_FORMAT: &str = "bestvideo[ext=mp4][vcodec^=avc1]/bestvideo[ext=mp4]/bestvideo";
const DEFAULT_AUDIO_FORMAT: &str = "bestaudio/best";
const DEFAULT_FFMPEG: &str = "ffmpeg";
const DEFAULT_YTDLP: &str = "yt-dlp";
const DEFAULT_AUDIO_RATE: u32 = 44100;

#[derive(Clone)]
pub struct YoutubeSettings {
    pub url: Option<String>,
    pub input: Option<PathBuf>,
    pub width: u32,
    pub height: Option<u32>,
    pub fps: u32,
    pub yt_dlp: String,
    pub ffmpeg: String,
    pub video_format: String,
    pub audio_format: String,
    pub audio_sample_rate: u32,
    pub audio_enabled: bool,
}

impl YoutubeSettings {
    pub fn from_env(audio_override: Option<bool>) -> Self {
        let url = env::var("TUI_GPU_YOUTUBE_URL").ok();
        let input = env::var("TUI_GPU_YOUTUBE_INPUT").ok().map(PathBuf::from);
        let width = env::var("TUI_GPU_YOUTUBE_WIDTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(RGB_DEMO_WIDTH);
        let height = env::var("TUI_GPU_YOUTUBE_HEIGHT")
            .ok()
            .and_then(|v| v.parse().ok());
        let fps = env::var("TUI_GPU_YOUTUBE_FPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(24);
        let yt_dlp =
            env::var("TUI_GPU_YOUTUBE_YTDLP").unwrap_or_else(|_| DEFAULT_YTDLP.to_string());
        let ffmpeg =
            env::var("TUI_GPU_YOUTUBE_FFMPEG").unwrap_or_else(|_| DEFAULT_FFMPEG.to_string());
        let video_format =
            env::var("TUI_GPU_YOUTUBE_FORMAT").unwrap_or_else(|_| DEFAULT_VIDEO_FORMAT.to_string());
        let audio_format = env::var("TUI_GPU_YOUTUBE_AUDIO_FORMAT")
            .unwrap_or_else(|_| DEFAULT_AUDIO_FORMAT.to_string());
        let audio_sample_rate = env::var("TUI_GPU_YOUTUBE_AUDIO_RATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_AUDIO_RATE);
        let env_audio = env::var("TUI_GPU_YOUTUBE_AUDIO")
            .ok()
            .and_then(|v| v.parse::<bool>().ok());
        let audio_enabled = audio_override.or(env_audio).unwrap_or(true);
        Self {
            url,
            input,
            width,
            height,
            fps,
            yt_dlp,
            ffmpeg,
            video_format,
            audio_format,
            audio_sample_rate,
            audio_enabled,
        }
    }
}

pub struct YoutubeSource {
    frames: Receiver<Option<RgbFrame>>,
    _thread: thread::JoinHandle<()>,
    _audio: Option<YoutubeAudioHandle>,
}

impl YoutubeSource {
    pub fn new(settings: YoutubeSettings) -> Result<Self> {
        let (tx, rx) = sync_channel(2);
        let (video_source, audio_source, width, target_height) = resolve_sources(&settings)?;
        let ffmpeg_video = settings.ffmpeg.clone();
        let fps = settings.fps;
        let handle = thread::spawn(move || {
            let _ = run_video_loop(ffmpeg_video, video_source, width, target_height, fps, tx);
        });
        let audio_handle = if settings.audio_enabled {
            audio_source.and_then(|source| {
                spawn_audio_player(settings.ffmpeg.clone(), source, settings.audio_sample_rate)
            })
        } else {
            None
        };
        Ok(Self {
            frames: rx,
            _thread: handle,
            _audio: audio_handle,
        })
    }
}

impl crate::demos::RgbSource for YoutubeSource {
    fn update(&mut self, _dt: f32) -> Result<Option<RgbFrame>> {
        match self.frames.try_recv() {
            Ok(Some(frame)) => Ok(Some(frame)),
            Ok(None) => Ok(None),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("youtube decoder exited"),
        }
    }
}

fn run_video_loop(
    ffmpeg: String,
    source: MediaSource,
    width: u32,
    target_height: u32,
    fps: u32,
    tx: SyncSender<Option<RgbFrame>>,
) -> Result<()> {
    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-loglevel").arg("warning").arg("-nostdin");
    match &source {
        MediaSource::Url(url) => {
            cmd.arg("-i").arg(url);
        }
        MediaSource::File(path) => {
            cmd.arg("-i").arg(path);
        }
    }
    let scale = format!("scale={}:{}", width, target_height);
    cmd.arg("-vf")
        .arg(scale)
        .arg("-r")
        .arg(fps.to_string())
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgb24")
        .arg("-");
    cmd.stdout(Stdio::piped());
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning {}", ffmpeg))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("ffmpeg stdout unavailable"))?;
    let frame_bytes = (width * target_height * 3) as usize;
    loop {
        let mut buf = vec![0u8; frame_bytes];
        match stdout.read_exact(&mut buf) {
            Ok(()) => {
                let frame = RgbFrame::new(width, target_height, buf);
                if tx.send(Some(frame)).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = tx.send(None);
    Ok(())
}

fn resolve_sources(
    settings: &YoutubeSettings,
) -> Result<(MediaSource, Option<MediaSource>, u32, u32)> {
    let width = settings.width.max(1);
    let height = settings.height.unwrap_or_else(|| default_height(width));
    if let Some(path) = settings.input.clone() {
        let metadata =
            std::fs::metadata(&path).with_context(|| format!("stat {}", path.display()))?;
        if !metadata.is_file() {
            bail!("{} is not a file", path.display());
        }
        let video = MediaSource::File(path.clone());
        let audio = if settings.audio_enabled {
            Some(MediaSource::File(path))
        } else {
            None
        };
        return Ok((video, audio, width, height));
    }
    let url = settings
        .url
        .clone()
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let video_url = fetch_direct_url(&settings.yt_dlp, &url, &settings.video_format)?;
    let audio = if settings.audio_enabled {
        let audio_url = fetch_direct_url(&settings.yt_dlp, &url, &settings.audio_format)?;
        Some(MediaSource::Url(audio_url))
    } else {
        None
    };
    Ok((MediaSource::Url(video_url), audio, width, height))
}

fn fetch_direct_url(ytdlp: &str, url: &str, format: &str) -> Result<String> {
    let output = Command::new(ytdlp)
        .arg("--quiet")
        .arg("--no-warnings")
        .arg("-f")
        .arg(format)
        .arg("--get-url")
        .arg(url)
        .output()
        .with_context(|| format!("running {} to fetch video URL", ytdlp))?;
    if !output.status.success() {
        bail!("{} failed to resolve {}", ytdlp, url);
    }
    let direct = String::from_utf8(output.stdout)
        .context("yt-dlp output not UTF-8")?
        .lines()
        .next()
        .ok_or_else(|| anyhow!("yt-dlp returned no URLs"))?
        .trim()
        .to_string();
    Ok(direct)
}

#[derive(Clone)]
enum MediaSource {
    Url(String),
    File(PathBuf),
}

struct YoutubeAudioHandle {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    _stream: OutputStream,
}

impl Drop for YoutubeAudioHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn spawn_audio_player(
    ffmpeg: String,
    source: MediaSource,
    sample_rate: u32,
) -> Option<YoutubeAudioHandle> {
    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-loglevel").arg("warning").arg("-nostdin");
    match &source {
        MediaSource::Url(url) => {
            cmd.arg("-i").arg(url);
        }
        MediaSource::File(path) => {
            cmd.arg("-i").arg(path);
        }
    }
    cmd.arg("-vn")
        .arg("-sn")
        .arg("-f")
        .arg("s16le")
        .arg("-ac")
        .arg("2")
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-");
    cmd.stdout(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let (stream, handle) = OutputStream::try_default().ok()?;
    let sink = Sink::try_new(&handle).ok()?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let thread = thread::spawn(move || {
        run_audio_loop(stdout, child, sink, stop_clone, sample_rate);
    });
    Some(YoutubeAudioHandle {
        stop,
        thread: Some(thread),
        _stream: stream,
    })
}

fn run_audio_loop(
    mut pipe: ChildStdout,
    mut child: Child,
    sink: Sink,
    stop: Arc<AtomicBool>,
    sample_rate: u32,
) {
    let mut buf = vec![0u8; 8192];
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let samples = bytes_to_samples(&buf[..n]);
                if samples.is_empty() {
                    continue;
                }
                let buffer = SamplesBuffer::new(2, sample_rate, samples);
                sink.append(buffer);
            }
            Err(err) => {
                eprintln!("youtube audio read error: {err:?}");
                break;
            }
        }
    }
    sink.stop();
    let _ = child.kill();
    let _ = child.wait();
}

fn bytes_to_samples(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    out
}

fn default_height(width: u32) -> u32 {
    let aspect = RGB_DEMO_HEIGHT as f32 / RGB_DEMO_WIDTH as f32;
    (width as f32 * aspect).round().max(1.0) as u32
}
