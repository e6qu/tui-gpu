use std::{
    io::{self, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

use ansi_image::{convert_image_to_ansi, DEFAULT_CELL_ASPECT, DEFAULT_PALETTE};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use frame_feed::{FrameFeedWriter, DEFAULT_FEED_CAPACITY, DEFAULT_FEED_PATH};
use image::imageops::FilterType;

#[derive(Parser)]
#[command(author, version, about = "TUI demo suite (image + ray cube)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert a PNG/JPEG into ANSI-colored ASCII
    Image {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 80)]
        width: u32,
        #[arg(long)]
        height: Option<u32>,
    },
    /// Stream a spinning ray-marched cube as ANSI
    Cube {
        #[arg(long, default_value_t = 100)]
        width: u32,
        #[arg(long, default_value_t = 40)]
        height: u32,
        #[arg(long, default_value_t = 600)]
        frames: u32,
        #[arg(long, default_value_t = 33)]
        sleep: u64,
    },
    /// Stream a PNG/JPEG into the shared RGB frame feed
    FeedImage {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long, default_value_t = 1)]
        interval: u64,
    },
    /// Stream a ray-marched cube into the shared RGB frame feed
    FeedCube {
        #[arg(long, default_value_t = 160)]
        width: u32,
        #[arg(long, default_value_t = 90)]
        height: u32,
        #[arg(long, default_value_t = 30)]
        fps: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Image {
            input,
            width,
            height,
        } => run_image(input, width, height),
        Commands::Cube {
            width,
            height,
            frames,
            sleep,
        } => run_cube(width, height, frames, sleep),
        Commands::FeedImage {
            input,
            width,
            height,
            interval,
        } => run_feed_image(input, width, height, interval),
        Commands::FeedCube { width, height, fps } => run_feed_cube(width, height, fps),
    }
}

fn run_image(input: PathBuf, width: u32, height: Option<u32>) -> Result<()> {
    let image = image::io::Reader::open(&input)
        .with_context(|| format!("opening {}", input.display()))?
        .decode()
        .with_context(|| format!("decoding {}", input.display()))?;
    let palette: Vec<char> = DEFAULT_PALETTE.chars().collect();
    let art = convert_image_to_ansi(&image, width, height, &palette, DEFAULT_CELL_ASPECT)?;
    print!("{art}\x1b[0m");
    Ok(())
}

fn run_cube(width: u32, height: u32, frames: u32, sleep_ms: u64) -> Result<()> {
    let palette: Vec<char> = DEFAULT_PALETTE.chars().collect();
    print!("\x1b[2J");
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for frame in 0..frames {
        let angle = frame as f32 * 0.05;
        let art = render_cube_frame(width, height, angle, &palette);
        write!(handle, "\x1b[H{art}")?;
        handle.flush()?;
        thread::sleep(Duration::from_millis(sleep_ms));
    }
    write!(handle, "\x1b[0m")?;
    Ok(())
}

fn run_feed_image(input: PathBuf, width: u32, height: u32, interval_ms: u64) -> Result<()> {
    let path = frame_feed_path();
    let mut writer = FrameFeedWriter::open(&path, DEFAULT_FEED_CAPACITY)?;
    let pixels = load_rgb_image(&input, width, height)?;
    let delay = Duration::from_millis(interval_ms.max(1));
    loop {
        writer.write_frame(width, height, &pixels)?;
        thread::sleep(delay);
    }
}

fn run_feed_cube(width: u32, height: u32, fps: u32) -> Result<()> {
    let path = frame_feed_path();
    let mut writer = FrameFeedWriter::open(&path, DEFAULT_FEED_CAPACITY)?;
    let sleep = Duration::from_millis((1000 / fps.max(1)) as u64);
    let mut angle = 0.0;
    loop {
        let frame = render_cube_rgb_frame(width, height, angle);
        writer.write_frame(width, height, &frame)?;
        angle += 0.05;
        thread::sleep(sleep);
    }
}

fn load_rgb_image(path: &PathBuf, width: u32, height: u32) -> Result<Vec<u8>> {
    let img = image::io::Reader::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .decode()
        .with_context(|| format!("decoding {}", path.display()))?;
    let resized = img
        .resize_exact(width, height, FilterType::Triangle)
        .to_rgb8();
    Ok(resized.into_raw())
}

fn frame_feed_path() -> String {
    std::env::var("TUI_GPU_FRAME_FEED").unwrap_or_else(|_| DEFAULT_FEED_PATH.to_string())
}

fn render_cube_frame(width: u32, height: u32, angle: f32, palette: &[char]) -> String {
    let mut output = String::new();
    let mut current_color: Option<(u8, u8, u8)> = None;
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    for y in 0..height {
        for x in 0..width {
            let u = if width == 1 {
                0.0
            } else {
                (x as f32 / (width_f - 1.0)) * 2.0 - 1.0
            };
            let v = if height == 1 {
                0.0
            } else {
                (y as f32 / (height_f - 1.0)) * 2.0 - 1.0
            };
            if let Some((ch, color)) = shade_pixel(u * (width_f / height_f), -v, angle, palette) {
                if current_color != Some(color) {
                    output.push_str(&format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2));
                    current_color = Some(color);
                }
                output.push(ch);
            } else {
                if current_color.is_some() {
                    output.push_str("\x1b[0m");
                    current_color = None;
                }
                output.push(' ');
            }
        }
        if current_color.is_some() {
            output.push_str("\x1b[0m");
            current_color = None;
        }
        output.push('\n');
    }
    output
}

fn render_cube_rgb_frame(width: u32, height: u32, angle: f32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize * height as usize * 3) as usize];
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    for y in 0..height {
        for x in 0..width {
            let u = if width == 1 {
                0.0
            } else {
                (x as f32 / (width_f - 1.0)) * 2.0 - 1.0
            };
            let v = if height == 1 {
                0.0
            } else {
                (y as f32 / (height_f - 1.0)) * 2.0 - 1.0
            };
            let idx = ((y * width + x) * 3) as usize;
            if let Some(sample) = cube_shade(u * (width_f / height_f), -v, angle) {
                pixels[idx] = sample.color.0;
                pixels[idx + 1] = sample.color.1;
                pixels[idx + 2] = sample.color.2;
            } else {
                pixels[idx] = 0;
                pixels[idx + 1] = 0;
                pixels[idx + 2] = 0;
            }
        }
    }
    pixels
}

fn shade_pixel(u: f32, v: f32, angle: f32, palette: &[char]) -> Option<(char, (u8, u8, u8))> {
    let sample = cube_shade(u, v, angle)?;
    let idx = ((palette.len().saturating_sub(1)) as f32 * sample.intensity)
        .round()
        .clamp(0.0, (palette.len().saturating_sub(1)) as f32) as usize;
    Some((palette[idx], sample.color))
}

#[derive(Clone, Copy)]
struct ShadeSample {
    intensity: f32,
    color: (u8, u8, u8),
}

fn cube_shade(u: f32, v: f32, angle: f32) -> Option<ShadeSample> {
    let cam_origin = Vec3::new(0.0, 0.0, 3.0);
    let mut dir = Vec3::new(u, v * 0.8, -1.5).normalize();
    let inv_angle_y = -angle * 0.7;
    let inv_angle_x = -angle * 0.4;
    dir = rotate(dir, inv_angle_x, inv_angle_y).normalize();
    let origin = rotate(cam_origin, inv_angle_x, inv_angle_y);
    let (t, normal) = intersect_cube(origin, dir)?;
    let hit_point = origin + dir * t;
    let light_dir = Vec3::new(0.3, 0.7, -1.0).normalize();
    let mut intensity = normal.dot(light_dir).max(0.0);
    intensity = (intensity * 0.85 + 0.15).clamp(0.0, 1.0);
    let base = Vec3::new(
        normal.x.abs() * 0.6 + 0.3 + (hit_point.y * 0.15).sin() * 0.1,
        normal.y.abs() * 0.6 + 0.3 + (hit_point.x * 0.15).cos() * 0.1,
        normal.z.abs() * 0.6 + 0.3 + (angle * 0.4).sin() * 0.1,
    );
    let color_vec = base * intensity;
    let color = (
        (color_vec.x.clamp(0.0, 1.0) * 255.0) as u8,
        (color_vec.y.clamp(0.0, 1.0) * 255.0) as u8,
        (color_vec.z.clamp(0.0, 1.0) * 255.0) as u8,
    );
    Some(ShadeSample { intensity, color })
}

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalize(self) -> Self {
        let len = self.length().max(1e-5);
        self * (1.0 / len)
    }
}

use std::ops::{Add, Mul, Sub};

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Vec3::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

fn rotate(v: Vec3, angle_x: f32, angle_y: f32) -> Vec3 {
    let siny = angle_y.sin();
    let cosy = angle_y.cos();
    let mut result = Vec3::new(v.x * cosy + v.z * siny, v.y, -v.x * siny + v.z * cosy);
    let sinx = angle_x.sin();
    let cosx = angle_x.cos();
    result = Vec3::new(
        result.x,
        result.y * cosx - result.z * sinx,
        result.y * sinx + result.z * cosx,
    );
    result
}

fn intersect_cube(origin: Vec3, dir: Vec3) -> Option<(f32, Vec3)> {
    let (tx0, tx1) = slab(origin.x, dir.x, -1.0, 1.0)?;
    let (ty0, ty1) = slab(origin.y, dir.y, -1.0, 1.0)?;
    let (tz0, tz1) = slab(origin.z, dir.z, -1.0, 1.0)?;
    let tmin = tx0.max(ty0).max(tz0);
    let tmax = tx1.min(ty1).min(tz1);
    if tmax < 0.0 || tmin > tmax {
        return None;
    }
    let t_hit = if tmin >= 0.0 { tmin } else { tmax };
    let hit = origin + dir * t_hit;
    let normal = surface_normal(hit);
    Some((t_hit, normal))
}

fn slab(origin: f32, dir: f32, min: f32, max: f32) -> Option<(f32, f32)> {
    if dir.abs() < 1e-4 {
        if origin < min || origin > max {
            None
        } else {
            Some((f32::NEG_INFINITY, f32::INFINITY))
        }
    } else {
        let inv = 1.0 / dir;
        let mut t0 = (min - origin) * inv;
        let mut t1 = (max - origin) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        Some((t0, t1))
    }
}

fn surface_normal(hit: Vec3) -> Vec3 {
    let abs_x = hit.x.abs();
    let abs_y = hit.y.abs();
    let abs_z = hit.z.abs();
    let max_axis = abs_x.max(abs_y).max(abs_z);
    if (max_axis - abs_x).abs() < 1e-3 {
        Vec3::new(hit.x.signum(), 0.0, 0.0)
    } else if (max_axis - abs_y).abs() < 1e-3 {
        Vec3::new(0.0, hit.y.signum(), 0.0)
    } else {
        Vec3::new(0.0, 0.0, hit.z.signum())
    }
}
