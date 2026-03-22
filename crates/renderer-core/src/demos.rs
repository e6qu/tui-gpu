use anyhow::{Context, Result};
use frame_feed::{FrameFeedReader, DEFAULT_FEED_CAPACITY, DEFAULT_FEED_PATH};

use crate::rgb_frame::RgbFrame;

pub trait RgbSource {
    fn update(&mut self, dt: f32) -> Result<Option<RgbFrame>>;
}

pub struct PlasmaSource {
    width: u32,
    height: u32,
    time: f32,
    speed: f32,
}

impl PlasmaSource {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            time: 0.0,
            speed: 1.0,
        }
    }
}

impl RgbSource for PlasmaSource {
    fn update(&mut self, dt: f32) -> Result<Option<RgbFrame>> {
        self.time += dt * self.speed;
        let mut pixels = vec![0u8; (self.width * self.height * 3) as usize];
        let width = self.width as f32;
        let height = self.height as f32;
        for y in 0..self.height {
            for x in 0..self.width {
                let xf = x as f32 / width;
                let yf = y as f32 / height;
                let value = (xf * 10.0 + self.time).sin()
                    + (yf * 10.0 - self.time * 0.5).cos()
                    + ((xf * xf + yf * yf).sqrt() * 8.0 + self.time * 0.7).sin();
                let normalized = ((value * 0.5) + 0.5).clamp(0.0, 1.0);
                let r = (normalized * 255.0) as u8;
                let g = ((normalized * 0.8 + 0.2) * 255.0) as u8;
                let b = (((1.0 - normalized) * 0.7 + 0.3) * 255.0) as u8;
                let idx = ((y * self.width + x) * 3) as usize;
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
            }
        }
        Ok(Some(RgbFrame::new(self.width, self.height, pixels)))
    }
}

pub struct RayDemoSource {
    width: u32,
    height: u32,
    time: f32,
}

impl RayDemoSource {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            time: 0.0,
        }
    }
}

impl RgbSource for RayDemoSource {
    fn update(&mut self, dt: f32) -> Result<Option<RgbFrame>> {
        self.time += dt;
        let mut pixels = vec![0u8; (self.width * self.height * 3) as usize];
        let width = self.width as f32;
        let height = self.height as f32;
        let aspect = width / height.max(1.0);
        for y in 0..self.height {
            for x in 0..self.width {
                let mut u = (x as f32 / width) * 2.0 - 1.0;
                let mut v = (y as f32 / height) * 2.0 - 1.0;
                u *= aspect;
                v = -v;
                let dir = Vec3::new(u, v, -1.2).normalize();
                let color = shade_ray(dir, self.time);
                let idx = ((y * self.width + x) * 3) as usize;
                pixels[idx] = color.0;
                pixels[idx + 1] = color.1;
                pixels[idx + 2] = color.2;
            }
        }
        Ok(Some(RgbFrame::new(self.width, self.height, pixels)))
    }
}

fn shade_ray(dir: Vec3, time: f32) -> (u8, u8, u8) {
    let origin = Vec3::new(0.0, 0.0, 0.0);
    let light_dir = Vec3::new(0.4, 0.8, -0.5).normalize();
    let sphere_center = Vec3::new(
        (time * 0.8).sin() * 1.5,
        (time * 0.5).cos() * 0.5,
        -4.0 - (time * 0.35).cos(),
    );
    if let Some((_, normal)) = intersect_sphere(origin, dir, sphere_center, 1.2) {
        let diffuse = normal.dot(light_dir).max(0.0);
        let spec_dir = reflect(-light_dir, normal);
        let spec = spec_dir.dot(-dir).max(0.0).powf(32.0);
        let base = Vec3::new(0.8, 0.2, 0.3);
        let color = base * diffuse + Vec3::splat(spec) + Vec3::splat(0.1);
        return to_rgb(color);
    }
    if let Some((t, normal)) = intersect_plane(
        origin,
        dir,
        Vec3::new(0.0, -1.2, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ) {
        let hit = origin + dir * t;
        let checker = ((hit.x * 1.5).floor() + (hit.z * 1.5).floor()) % 2.0;
        let base = Vec3::new(0.2, 0.2, 0.25).mix(Vec3::new(0.9, 0.9, 0.95), checker);
        let diffuse = normal.dot(light_dir).max(0.0);
        let color = base * (diffuse * 0.8 + 0.2);
        return to_rgb(color);
    }
    let t = 0.5 * (dir.y + 1.0);
    let top = Vec3::new(0.2, 0.3, 0.6);
    let bottom = Vec3::new(0.05, 0.05, 0.1);
    to_rgb(top.mix(bottom, t))
}

fn intersect_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<(f32, Vec3)> {
    let oc = origin - center;
    let a = dir.dot(dir);
    let b = 2.0 * oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let mut t = (-b - sq) / (2.0 * a);
    if t < 0.0 {
        t = (-b + sq) / (2.0 * a);
    }
    if t < 0.0 {
        return None;
    }
    let hit = origin + dir * t;
    let normal = (hit - center).normalize();
    Some((t, normal))
}

fn intersect_plane(origin: Vec3, dir: Vec3, point: Vec3, normal: Vec3) -> Option<(f32, Vec3)> {
    let denom = normal.dot(dir);
    if denom.abs() < 1e-4 {
        return None;
    }
    let t = (point - origin).dot(normal) / denom;
    if t < 0.0 {
        return None;
    }
    Some((t, normal))
}

fn reflect(v: Vec3, normal: Vec3) -> Vec3 {
    v - normal * (2.0 * v.dot(normal))
}

fn to_rgb(color: Vec3) -> (u8, u8, u8) {
    (
        (color.x.clamp(0.0, 1.0) * 255.0) as u8,
        (color.y.clamp(0.0, 1.0) * 255.0) as u8,
        (color.z.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

pub struct DoomFeedSource {
    reader: FrameFeedReader,
}

impl DoomFeedSource {
    pub fn new() -> Result<Self> {
        let path =
            std::env::var("TUI_GPU_FRAME_FEED").unwrap_or_else(|_| DEFAULT_FEED_PATH.to_string());
        let reader = FrameFeedReader::open(&path, DEFAULT_FEED_CAPACITY)
            .with_context(|| format!("failed to open frame feed at {}", path))?;
        Ok(Self { reader })
    }
}

impl RgbSource for DoomFeedSource {
    fn update(&mut self, _dt: f32) -> Result<Option<RgbFrame>> {
        match self.reader.poll_frame()? {
            Some(frame) => Ok(Some(RgbFrame::new(frame.width, frame.height, frame.pixels))),
            None => Ok(None),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn splat(v: f32) -> Self {
        Self { x: v, y: v, z: v }
    }

    fn dot(self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalize(self) -> Self {
        let len = self.length().max(1e-5);
        self * (1.0 / len)
    }

    fn mix(self, other: Vec3, t: f32) -> Vec3 {
        self * (1.0 - t) + other * t
    }
}

use std::ops::{Add, Mul, Neg, Sub};

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, rhs: f32) -> Vec3 {
        Vec3::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}
