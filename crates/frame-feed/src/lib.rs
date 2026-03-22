use anyhow::{bail, Context, Result};
use memmap2::{Mmap, MmapMut};
use std::{
    fs::OpenOptions,
    path::Path,
    sync::atomic::{fence, Ordering},
};

pub const DEFAULT_FEED_PATH: &str = "/tmp/tui_gpu_framefeed";
pub const DEFAULT_FEED_CAPACITY: usize = 16 * 1024 * 1024;
pub const DEFAULT_INPUT_PATH: &str = "/tmp/tui_gpu_inputfeed";
pub const DEFAULT_INPUT_CAPACITY: usize = 4096;
pub const DEFAULT_AUDIO_PATH: &str = "/tmp/tui_gpu_audiofeed";
pub const DEFAULT_AUDIO_CAPACITY: usize = 4 * 1024 * 1024;

const MAGIC: [u8; 4] = *b"RBF1";
const WIDTH_OFFSET: usize = 4;
const HEIGHT_OFFSET: usize = 8;
const LEN_OFFSET: usize = 12;
const GENERATION_OFFSET: usize = 16;
const HEADER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct FrameData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct FrameFeedWriter {
    mmap: MmapMut,
    capacity: usize,
    generation: u64,
}

impl FrameFeedWriter {
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self> {
        if capacity < HEADER_SIZE {
            bail!("frame feed capacity {} too small", capacity);
        }
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        file.set_len(capacity as u64)
            .with_context(|| format!("resizing {}", path.display()))?;
        let mut mmap = unsafe { MmapMut::map_mut(&file).context("mapping frame feed writer")? };
        if &mmap[0..MAGIC.len()] != MAGIC {
            mmap[0..MAGIC.len()].copy_from_slice(&MAGIC);
            mmap[WIDTH_OFFSET..WIDTH_OFFSET + 4].fill(0);
            mmap[HEIGHT_OFFSET..HEIGHT_OFFSET + 4].fill(0);
            mmap[LEN_OFFSET..LEN_OFFSET + 4].fill(0);
            mmap[GENERATION_OFFSET..GENERATION_OFFSET + 8].fill(0);
        }
        Ok(Self {
            mmap,
            capacity,
            generation: 0,
        })
    }

    pub fn write_frame(&mut self, width: u32, height: u32, rgb: &[u8]) -> Result<()> {
        if width == 0 || height == 0 {
            bail!("frame dimensions must be >0");
        }
        let expected = width as usize * height as usize * 3;
        if expected != rgb.len() {
            bail!(
                "pixel data length {} does not match {}x{} frame (expected {})",
                rgb.len(),
                width,
                height,
                expected
            );
        }
        let total = HEADER_SIZE + rgb.len();
        if total > self.capacity {
            bail!(
                "frame of {} bytes exceeds feed capacity {}",
                total,
                self.capacity
            );
        }
        self.mmap[0..MAGIC.len()].copy_from_slice(&MAGIC);
        self.mmap[WIDTH_OFFSET..WIDTH_OFFSET + 4].copy_from_slice(&width.to_le_bytes());
        self.mmap[HEIGHT_OFFSET..HEIGHT_OFFSET + 4].copy_from_slice(&height.to_le_bytes());
        self.mmap[LEN_OFFSET..LEN_OFFSET + 4].copy_from_slice(&(rgb.len() as u32).to_le_bytes());
        self.mmap[HEADER_SIZE..HEADER_SIZE + rgb.len()].copy_from_slice(rgb);
        fence(Ordering::SeqCst);
        self.generation = self.generation.wrapping_add(1);
        self.mmap[GENERATION_OFFSET..GENERATION_OFFSET + 8]
            .copy_from_slice(&self.generation.to_le_bytes());
        let _ = self.mmap.flush_async();
        Ok(())
    }
}

pub struct FrameFeedReader {
    mmap: Mmap,
    last_generation: u64,
}

impl FrameFeedReader {
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self> {
        let path = path.as_ref();
        if capacity < HEADER_SIZE {
            bail!("frame feed capacity {} too small", capacity);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(false)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file).context("mapping frame feed reader")? };
        if mmap.len() < HEADER_SIZE {
            bail!(
                "frame feed file {} too small ({} bytes)",
                path.display(),
                mmap.len()
            );
        }
        if &mmap[0..MAGIC.len()] != MAGIC {
            bail!("frame feed {} missing magic header", path.display());
        }
        let last_generation = read_u64(&mmap, GENERATION_OFFSET);
        Ok(Self {
            mmap,
            last_generation,
        })
    }

    pub fn poll_frame(&mut self) -> Result<Option<FrameData>> {
        let generation = read_u64(&self.mmap, GENERATION_OFFSET);
        if generation == self.last_generation {
            return Ok(None);
        }
        let width = read_u32(&self.mmap, WIDTH_OFFSET);
        let height = read_u32(&self.mmap, HEIGHT_OFFSET);
        let len = read_u32(&self.mmap, LEN_OFFSET) as usize;
        if width == 0 || height == 0 || len == 0 {
            self.last_generation = generation;
            return Ok(None);
        }
        if HEADER_SIZE + len > self.mmap.len() {
            bail!("frame length {} exceeds mmap size {}", len, self.mmap.len());
        }
        if width as usize * height as usize * 3 != len {
            bail!(
                "frame len {} does not match {}x{} dimensions",
                len,
                width,
                height
            );
        }
        let mut pixels = vec![0u8; len];
        pixels.copy_from_slice(&self.mmap[HEADER_SIZE..HEADER_SIZE + len]);
        let generation_check = read_u64(&self.mmap, GENERATION_OFFSET);
        if generation_check != generation {
            self.last_generation = generation_check;
            return Ok(None);
        }
        self.last_generation = generation;
        Ok(Some(FrameData {
            width,
            height,
            pixels,
        }))
    }
}

fn read_u32(mmap: &Mmap, offset: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&mmap[offset..offset + 4]);
    u32::from_le_bytes(buf)
}

fn read_u64(mmap: &Mmap, offset: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&mmap[offset..offset + 8]);
    u64::from_le_bytes(buf)
}

const INPUT_MAGIC: [u8; 4] = *b"KBD1";
const INPUT_LEN_OFFSET: usize = 4;
const INPUT_GENERATION_OFFSET: usize = 8;
const INPUT_HEADER_SIZE: usize = 32;

pub struct InputFeedWriter {
    mmap: MmapMut,
    capacity: usize,
    generation: u64,
}

impl InputFeedWriter {
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self> {
        if capacity < INPUT_HEADER_SIZE {
            bail!("input feed capacity {} too small", capacity);
        }
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        file.set_len(capacity as u64)
            .with_context(|| format!("resizing {}", path.display()))?;
        let mut mmap = unsafe { MmapMut::map_mut(&file).context("mapping input feed writer")? };
        if &mmap[0..INPUT_MAGIC.len()] != INPUT_MAGIC {
            mmap[0..INPUT_MAGIC.len()].copy_from_slice(&INPUT_MAGIC);
            mmap[INPUT_LEN_OFFSET..INPUT_LEN_OFFSET + 4].fill(0);
            mmap[INPUT_GENERATION_OFFSET..INPUT_GENERATION_OFFSET + 8].fill(0);
        }
        Ok(Self {
            mmap,
            capacity,
            generation: 0,
        })
    }

    pub fn write_event(&mut self, data: &[u8]) -> Result<()> {
        let payload = self.capacity.saturating_sub(INPUT_HEADER_SIZE);
        if data.len() > payload {
            bail!(
                "event of {} bytes exceeds input feed payload {}",
                data.len(),
                payload
            );
        }
        self.mmap[0..INPUT_MAGIC.len()].copy_from_slice(&INPUT_MAGIC);
        self.mmap[INPUT_LEN_OFFSET..INPUT_LEN_OFFSET + 4]
            .copy_from_slice(&(data.len() as u32).to_le_bytes());
        self.mmap[INPUT_HEADER_SIZE..INPUT_HEADER_SIZE + data.len()].copy_from_slice(data);
        fence(Ordering::SeqCst);
        self.generation = self.generation.wrapping_add(1);
        self.mmap[INPUT_GENERATION_OFFSET..INPUT_GENERATION_OFFSET + 8]
            .copy_from_slice(&self.generation.to_le_bytes());
        let _ = self.mmap.flush_async();
        Ok(())
    }
}

pub struct InputFeedReader {
    mmap: Mmap,
    last_generation: u64,
}

impl InputFeedReader {
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self> {
        if capacity < INPUT_HEADER_SIZE {
            bail!("input feed capacity {} too small", capacity);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(false)
            .open(path.as_ref())
            .with_context(|| format!("opening {}", path.as_ref().display()))?;
        let mmap = unsafe { Mmap::map(&file).context("mapping input feed reader")? };
        if mmap.len() < INPUT_HEADER_SIZE {
            bail!(
                "input feed file {} too small ({} bytes)",
                path.as_ref().display(),
                mmap.len()
            );
        }
        if &mmap[0..INPUT_MAGIC.len()] != INPUT_MAGIC {
            bail!(
                "input feed {} missing magic header",
                path.as_ref().display()
            );
        }
        let last_generation = read_u64(&mmap, INPUT_GENERATION_OFFSET);
        Ok(Self {
            mmap,
            last_generation,
        })
    }

    pub fn poll_event(&mut self) -> Result<Option<Vec<u8>>> {
        let generation = read_u64(&self.mmap, INPUT_GENERATION_OFFSET);
        if generation == self.last_generation {
            return Ok(None);
        }
        let len = read_u32(&self.mmap, INPUT_LEN_OFFSET) as usize;
        if len == 0 {
            self.last_generation = generation;
            return Ok(None);
        }
        if INPUT_HEADER_SIZE + len > self.mmap.len() {
            bail!(
                "input event length {} exceeds mmap size {}",
                len,
                self.mmap.len()
            );
        }
        let mut data = vec![0u8; len];
        data.copy_from_slice(&self.mmap[INPUT_HEADER_SIZE..INPUT_HEADER_SIZE + len]);
        let gen_check = read_u64(&self.mmap, INPUT_GENERATION_OFFSET);
        if gen_check != generation {
            self.last_generation = gen_check;
            return Ok(None);
        }
        self.last_generation = generation;
        Ok(Some(data))
    }
}

const AUDIO_MAGIC: [u8; 4] = *b"AUD1";
const AUDIO_RATE_OFFSET: usize = 4;
const AUDIO_COUNT_OFFSET: usize = 8;
const AUDIO_LEN_OFFSET: usize = 12;
const AUDIO_VOLUME_OFFSET: usize = 16;
const AUDIO_SEP_OFFSET: usize = 17;
const AUDIO_FLAGS_OFFSET: usize = 18;
const AUDIO_GENERATION_OFFSET: usize = 20;
const AUDIO_HEADER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct AudioEvent {
    pub sample_rate: u32,
    pub volume: u8,
    pub separation: u8,
    pub samples: Vec<i16>,
}

pub struct AudioFeedWriter {
    mmap: MmapMut,
    capacity: usize,
    generation: u64,
}

impl AudioFeedWriter {
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self> {
        if capacity < AUDIO_HEADER_SIZE {
            bail!("audio feed capacity {} too small", capacity);
        }
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        file.set_len(capacity as u64)
            .with_context(|| format!("resizing {}", path.display()))?;
        let mut mmap = unsafe { MmapMut::map_mut(&file).context("mapping audio feed writer")? };
        if &mmap[0..AUDIO_MAGIC.len()] != AUDIO_MAGIC {
            mmap[0..AUDIO_MAGIC.len()].copy_from_slice(&AUDIO_MAGIC);
            mmap[AUDIO_RATE_OFFSET..AUDIO_RATE_OFFSET + 4].fill(0);
            mmap[AUDIO_COUNT_OFFSET..AUDIO_COUNT_OFFSET + 4].fill(0);
            mmap[AUDIO_LEN_OFFSET..AUDIO_LEN_OFFSET + 4].fill(0);
            mmap[AUDIO_VOLUME_OFFSET] = 0;
            mmap[AUDIO_SEP_OFFSET] = 0;
            mmap[AUDIO_FLAGS_OFFSET..AUDIO_FLAGS_OFFSET + 2].fill(0);
            mmap[AUDIO_GENERATION_OFFSET..AUDIO_GENERATION_OFFSET + 8].fill(0);
        }
        Ok(Self {
            mmap,
            capacity,
            generation: 0,
        })
    }

    pub fn write_event(
        &mut self,
        sample_rate: u32,
        volume: u8,
        separation: u8,
        samples: &[i16],
    ) -> Result<()> {
        if sample_rate == 0 || samples.is_empty() {
            bail!("audio event must have rate and samples");
        }
        let data_len = samples.len() * std::mem::size_of::<i16>();
        let total = AUDIO_HEADER_SIZE + data_len;
        if total > self.capacity {
            bail!(
                "audio payload of {} bytes exceeds feed capacity {}",
                total,
                self.capacity
            );
        }
        self.mmap[0..AUDIO_MAGIC.len()].copy_from_slice(&AUDIO_MAGIC);
        self.mmap[AUDIO_RATE_OFFSET..AUDIO_RATE_OFFSET + 4]
            .copy_from_slice(&sample_rate.to_le_bytes());
        let sample_count = samples.len() as u32;
        self.mmap[AUDIO_COUNT_OFFSET..AUDIO_COUNT_OFFSET + 4]
            .copy_from_slice(&sample_count.to_le_bytes());
        self.mmap[AUDIO_LEN_OFFSET..AUDIO_LEN_OFFSET + 4]
            .copy_from_slice(&(data_len as u32).to_le_bytes());
        self.mmap[AUDIO_VOLUME_OFFSET] = volume;
        self.mmap[AUDIO_SEP_OFFSET] = separation;
        self.mmap[AUDIO_FLAGS_OFFSET..AUDIO_FLAGS_OFFSET + 2].fill(0);
        let bytes: &[u8] = bytemuck::cast_slice(samples);
        self.mmap[AUDIO_HEADER_SIZE..AUDIO_HEADER_SIZE + data_len].copy_from_slice(bytes);
        fence(Ordering::SeqCst);
        self.generation = self.generation.wrapping_add(1);
        self.mmap[AUDIO_GENERATION_OFFSET..AUDIO_GENERATION_OFFSET + 8]
            .copy_from_slice(&self.generation.to_le_bytes());
        let _ = self.mmap.flush_async();
        Ok(())
    }
}

pub struct AudioFeedReader {
    mmap: Mmap,
    last_generation: u64,
}

impl AudioFeedReader {
    pub fn open(path: impl AsRef<Path>, capacity: usize) -> Result<Self> {
        if capacity < AUDIO_HEADER_SIZE {
            bail!("audio feed capacity {} too small", capacity);
        }
        let path = path.as_ref();
        let file = OpenOptions::new()
            .read(true)
            .write(false)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        let mmap = unsafe { Mmap::map(&file).context("mapping audio feed reader")? };
        if mmap.len() < AUDIO_HEADER_SIZE {
            bail!(
                "audio feed file {} too small ({} bytes)",
                path.display(),
                mmap.len()
            );
        }
        if &mmap[0..AUDIO_MAGIC.len()] != AUDIO_MAGIC {
            bail!("audio feed {} missing magic header", path.display());
        }
        let last_generation = read_u64(&mmap, AUDIO_GENERATION_OFFSET);
        Ok(Self {
            mmap,
            last_generation,
        })
    }

    pub fn poll_event(&mut self) -> Result<Option<AudioEvent>> {
        let generation = read_u64(&self.mmap, AUDIO_GENERATION_OFFSET);
        if generation == self.last_generation {
            return Ok(None);
        }
        let sample_rate = read_u32(&self.mmap, AUDIO_RATE_OFFSET);
        let sample_count = read_u32(&self.mmap, AUDIO_COUNT_OFFSET) as usize;
        let len = read_u32(&self.mmap, AUDIO_LEN_OFFSET) as usize;
        if sample_rate == 0 || len == 0 || sample_count == 0 {
            self.last_generation = generation;
            return Ok(None);
        }
        if AUDIO_HEADER_SIZE + len > self.mmap.len() {
            bail!(
                "audio payload length {} exceeds mmap size {}",
                len,
                self.mmap.len()
            );
        }
        if sample_count * 2 != len {
            bail!(
                "audio len {} does not match sample count {}",
                len,
                sample_count
            );
        }
        let mut samples = vec![0i16; sample_count];
        let bytes = &self.mmap[AUDIO_HEADER_SIZE..AUDIO_HEADER_SIZE + len];
        let src: &[i16] = bytemuck::cast_slice(bytes);
        samples.copy_from_slice(src);
        let generation_check = read_u64(&self.mmap, AUDIO_GENERATION_OFFSET);
        if generation_check != generation {
            self.last_generation = generation_check;
            return Ok(None);
        }
        self.last_generation = generation;
        Ok(Some(AudioEvent {
            sample_rate,
            volume: self.mmap[AUDIO_VOLUME_OFFSET],
            separation: self.mmap[AUDIO_SEP_OFFSET],
            samples,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn writer_reader_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("feed.bin");
        let mut writer = FrameFeedWriter::open(&path, 256 * 1024).unwrap();
        let mut reader = FrameFeedReader::open(&path, 256 * 1024).unwrap();
        let width = 4;
        let height = 2;
        let mut pixels = Vec::new();
        for i in 0..(width * height) {
            pixels.extend_from_slice(&[i as u8, (i * 2) as u8, (i * 3) as u8]);
        }
        writer.write_frame(width, height, &pixels).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let frame = reader.poll_frame().unwrap().expect("frame");
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.pixels, pixels);
    }
    #[test]
    fn input_feed_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("input.bin");
        let mut writer = InputFeedWriter::open(&path, 4096).unwrap();
        let mut reader = InputFeedReader::open(&path, 4096).unwrap();
        writer.write_event(b"hello").unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let event = reader.poll_event().unwrap().expect("event");
        assert_eq!(event, b"hello");
    }

    #[test]
    fn audio_feed_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("audio.bin");
        let mut writer = AudioFeedWriter::open(&path, 64 * 1024).unwrap();
        let mut reader = AudioFeedReader::open(&path, 64 * 1024).unwrap();
        let samples: Vec<i16> = (0..64).map(|i| (i as i16) * 32).collect();
        writer.write_event(44100, 100, 200, &samples).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let event = reader.poll_event().unwrap().expect("event");
        assert_eq!(event.sample_rate, 44100);
        assert_eq!(event.volume, 100);
        assert_eq!(event.separation, 200);
        assert_eq!(event.samples, samples);
    }
}
