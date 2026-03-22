use anyhow::{bail, Result};
use image::{imageops::FilterType, DynamicImage};
use std::cmp::Ordering;

pub const DEFAULT_PALETTE: &str = " .:-=+*#%@";
pub const DENSE_PALETTE: &str =
    " .'`^\",:;Il!i~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
pub const BLOCKY_PALETTE: &str = " ░▒▓█";
pub const DEFAULT_CELL_ASPECT: f32 = 9.0 / 16.0;
const BRAILLE_CELL_ASPECT: f32 = DEFAULT_CELL_ASPECT;
const BLOCK_THRESHOLD: f32 = 0.2;
const BRAILLE_BITS: [u8; 8] = [0, 1, 2, 6, 3, 4, 5, 7];

/// Converts the provided image into ANSI-colored ASCII art sized for terminal grids.
pub fn convert_image_to_ansi(
    image: &DynamicImage,
    width: u32,
    height: Option<u32>,
    palette: &[char],
    cell_aspect: f32,
) -> Result<String> {
    if palette.is_empty() {
        bail!("palette must contain at least one character");
    }
    let target_width = width.max(1);
    let aspect = image.height() as f32 / image.width() as f32;
    let height = height.unwrap_or_else(|| {
        let aspect_scale = cell_aspect.clamp(0.001, 100.0);
        let scaled = (aspect * aspect_scale * target_width as f32).max(1.0);
        scaled.round() as u32
    });

    let rgb = image.to_rgb8();
    let resized = image::imageops::resize(&rgb, target_width, height.max(1), FilterType::Triangle);

    let mut ansi = String::new();
    let mut current_color: Option<(u8, u8, u8)> = None;
    for y in 0..resized.height() {
        for x in 0..resized.width() {
            let pixel = resized.get_pixel(x, y);
            let level = luminance(pixel[0], pixel[1], pixel[2]);
            let idx = palette_index(level, palette.len());
            let rgb = (pixel[0], pixel[1], pixel[2]);
            if current_color != Some(rgb) {
                ansi.push_str(&format!("\x1b[38;2;{};{};{}m", rgb.0, rgb.1, rgb.2));
                current_color = Some(rgb);
            }
            ansi.push(palette[idx]);
        }
        ansi.push_str("\x1b[0m\n");
        current_color = None;
    }

    Ok(ansi)
}

/// Converts the provided image into ANSI-colored Unicode braille art for higher fidelity.
pub fn convert_image_to_braille(
    image: &DynamicImage,
    width: u32,
    height: Option<u32>,
) -> Result<String> {
    let width_chars = width.max(1);
    let aspect = image.height() as f32 / image.width() as f32;
    let height_chars = height.unwrap_or_else(|| {
        let scaled = (aspect * BRAILLE_CELL_ASPECT * width_chars as f32).max(1.0);
        scaled.round() as u32
    });
    let target_width = width_chars * 2;
    let target_height = height_chars.max(1) * 4;
    let rgb = image.to_rgb8();
    let resized = image::imageops::resize(&rgb, target_width, target_height, FilterType::Triangle);

    let mut output = String::new();
    let mut current_color: Option<(u8, u8, u8)> = None;

    for row in 0..height_chars {
        for col in 0..width_chars {
            let mut samples = [(0f32, [0u8; 3]); 8];
            for dy in 0..4 {
                for dx in 0..2 {
                    let px = resized.get_pixel(col * 2 + dx, row * 4 + dy);
                    let idx = (dy * 2 + dx) as usize;
                    let lum = luminance(px[0], px[1], px[2]) / 255.0;
                    samples[idx] = (lum, [px[0], px[1], px[2]]);
                }
            }
            let mut order = [0usize; 8];
            for (idx, slot) in order.iter_mut().enumerate() {
                *slot = idx;
            }
            order.sort_by(|a, b| {
                samples[*b]
                    .0
                    .partial_cmp(&samples[*a].0)
                    .unwrap_or(Ordering::Equal)
            });
            let average = samples.iter().map(|s| s.0).sum::<f32>() / 8.0;
            let mut count = (average * 8.0).round() as usize;
            count = count.clamp(0, 8);
            if count == 0 {
                if current_color.is_some() {
                    output.push_str("\x1b[0m");
                    current_color = None;
                }
                output.push(' ');
                continue;
            }
            let mut bits = 0u8;
            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            for idx in order.iter().take(count) {
                bits |= 1 << BRAILLE_BITS[*idx];
                sum_r += samples[*idx].1[0] as u32;
                sum_g += samples[*idx].1[1] as u32;
                sum_b += samples[*idx].1[2] as u32;
            }
            let count_u32 = count as u32;
            let color = (
                (sum_r / count_u32) as u8,
                (sum_g / count_u32) as u8,
                (sum_b / count_u32) as u8,
            );
            if current_color != Some(color) {
                output.push_str(&format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2));
                current_color = Some(color);
            }
            let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
            output.push(ch);
        }
        if current_color.is_some() {
            output.push_str("\x1b[0m");
            current_color = None;
        }
        output.push('\n');
    }
    Ok(output)
}

/// Converts the provided image into ANSI-colored Unicode block art (▀ ▄ █).
pub fn convert_image_to_blocks(
    image: &DynamicImage,
    width: u32,
    height: Option<u32>,
    cell_aspect: f32,
) -> Result<String> {
    let width_chars = width.max(1);
    let aspect = image.height() as f32 / image.width() as f32;
    let height_chars = height.unwrap_or_else(|| {
        let aspect_scale = cell_aspect.clamp(0.001, 100.0);
        let scaled = (aspect * aspect_scale * width_chars as f32).max(1.0);
        scaled.round() as u32
    });
    let target_height = height_chars.max(1) * 2;
    let rgb = image.to_rgb8();
    let resized = image::imageops::resize(&rgb, width_chars, target_height, FilterType::Triangle);
    let mut output = String::new();
    let mut current_color: Option<(u8, u8, u8)> = None;
    for row in 0..height_chars {
        for col in 0..width_chars {
            let upper = resized.get_pixel(col, row * 2);
            let lower =
                resized.get_pixel(col, (row * 2 + 1).min(resized.height().saturating_sub(1)));
            let upper_lum = luminance(upper[0], upper[1], upper[2]) / 255.0;
            let lower_lum = luminance(lower[0], lower[1], lower[2]) / 255.0;
            let glyph = match (upper_lum >= BLOCK_THRESHOLD, lower_lum >= BLOCK_THRESHOLD) {
                (false, false) => {
                    if current_color.is_some() {
                        output.push_str("\x1b[0m");
                        current_color = None;
                    }
                    output.push(' ');
                    continue;
                }
                (true, true) => {
                    let color = (
                        ((upper[0] as u16 + lower[0] as u16) / 2) as u8,
                        ((upper[1] as u16 + lower[1] as u16) / 2) as u8,
                        ((upper[2] as u16 + lower[2] as u16) / 2) as u8,
                    );
                    if current_color != Some(color) {
                        output.push_str(&format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2));
                        current_color = Some(color);
                    }
                    '█'
                }
                (true, false) => {
                    let color = (upper[0], upper[1], upper[2]);
                    if current_color != Some(color) {
                        output.push_str(&format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2));
                        current_color = Some(color);
                    }
                    '▀'
                }
                (false, true) => {
                    let color = (lower[0], lower[1], lower[2]);
                    if current_color != Some(color) {
                        output.push_str(&format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2));
                        current_color = Some(color);
                    }
                    '▄'
                }
            };
            output.push(glyph);
        }
        if current_color.is_some() {
            output.push_str("\x1b[0m");
            current_color = None;
        }
        output.push('\n');
    }
    Ok(output)
}

fn luminance(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
}

fn palette_index(level: f32, palette_len: usize) -> usize {
    if palette_len <= 1 {
        return 0;
    }
    let scaled = (level / 255.0) * (palette_len as f32 - 1.0);
    scaled.round().clamp(0.0, palette_len as f32 - 1.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    #[test]
    fn converts_to_ansi() {
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 2, |x, y| match (x, y) {
            (0, 0) => Rgb([255, 0, 0]),
            (1, 0) => Rgb([0, 255, 0]),
            (0, 1) => Rgb([0, 0, 255]),
            _ => Rgb([255, 255, 0]),
        }));
        let palette: Vec<char> = DEFAULT_PALETTE.chars().collect();
        let ansi = convert_image_to_ansi(&img, 2, Some(2), &palette, DEFAULT_CELL_ASPECT).unwrap();
        assert!(ansi.contains("\x1b[38;2;255;0;0m"));
        assert!(ansi.contains("\x1b[38;2;0;255;0m"));
        assert!(ansi.contains("\x1b[38;2;0;0;255m"));
    }

    #[test]
    fn braille_converter_outputs_unicode() {
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(2, 4, |_, _| Rgb([255, 255, 255])));
        let art = convert_image_to_braille(&img, 1, Some(1)).unwrap();
        assert!(art.chars().any(|c| c >= '\u{2800}' && c <= '\u{28FF}'));
    }

    #[test]
    fn block_converter_emits_block_chars() {
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(1, 2, |_, y| {
            if y == 0 {
                Rgb([255, 255, 255])
            } else {
                Rgb([0, 0, 0])
            }
        }));
        let art = convert_image_to_blocks(&img, 1, Some(1), DEFAULT_CELL_ASPECT).unwrap();
        assert!(art.contains('▀') || art.contains('▄') || art.contains('█'));
    }
}
