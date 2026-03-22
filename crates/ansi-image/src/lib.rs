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
const FULL_BLOCK_THRESHOLD: f32 = 0.55;
const HALF_BLOCK_THRESHOLD: f32 = 0.35;
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
    let mut current_fg: Option<(u8, u8, u8)> = None;
    let mut current_bg: Option<(u8, u8, u8)> = None;
    let set_fg = |color: (u8, u8, u8), out: &mut String, current: &mut Option<(u8, u8, u8)>| {
        if current.as_ref() != Some(&color) {
            out.push_str(&format!("\x1b[38;2;{};{};{}m", color.0, color.1, color.2));
            *current = Some(color);
        }
    };
    let set_bg = |color: (u8, u8, u8), out: &mut String, current: &mut Option<(u8, u8, u8)>| {
        if current.as_ref() != Some(&color) {
            out.push_str(&format!("\x1b[48;2;{};{};{}m", color.0, color.1, color.2));
            *current = Some(color);
        }
    };
    let reset_attrs =
        |out: &mut String, fg: &mut Option<(u8, u8, u8)>, bg: &mut Option<(u8, u8, u8)>| {
            if fg.is_some() || bg.is_some() {
                out.push_str("\x1b[0m");
                *fg = None;
                *bg = None;
            }
        };
    let reset_bg = |out: &mut String, bg: &mut Option<(u8, u8, u8)>| {
        if bg.is_some() {
            out.push_str("\x1b[49m");
            *bg = None;
        }
    };
    for row in 0..height_chars {
        for col in 0..width_chars {
            let upper = resized.get_pixel(col, row * 2);
            let lower =
                resized.get_pixel(col, (row * 2 + 1).min(resized.height().saturating_sub(1)));
            let upper_lum = luminance(upper[0], upper[1], upper[2]) / 255.0;
            let lower_lum = luminance(lower[0], lower[1], lower[2]) / 255.0;
            let upper_color = (upper[0], upper[1], upper[2]);
            let lower_color = (lower[0], lower[1], lower[2]);
            let avg_color = (
                ((upper[0] as u16 + lower[0] as u16) / 2) as u8,
                ((upper[1] as u16 + lower[1] as u16) / 2) as u8,
                ((upper[2] as u16 + lower[2] as u16) / 2) as u8,
            );
            let combined = (upper_lum + lower_lum) * 0.5;
            let glyph = if upper_lum >= FULL_BLOCK_THRESHOLD && lower_lum >= FULL_BLOCK_THRESHOLD {
                set_fg(avg_color, &mut output, &mut current_fg);
                reset_bg(&mut output, &mut current_bg);
                '█'
            } else if upper_lum >= FULL_BLOCK_THRESHOLD && lower_lum <= HALF_BLOCK_THRESHOLD {
                set_fg(upper_color, &mut output, &mut current_fg);
                if lower_lum > BLOCK_THRESHOLD {
                    set_bg(lower_color, &mut output, &mut current_bg);
                } else {
                    reset_bg(&mut output, &mut current_bg);
                }
                '▀'
            } else if lower_lum >= FULL_BLOCK_THRESHOLD && upper_lum <= HALF_BLOCK_THRESHOLD {
                if upper_lum > BLOCK_THRESHOLD {
                    set_bg(upper_color, &mut output, &mut current_bg);
                } else {
                    reset_bg(&mut output, &mut current_bg);
                }
                set_fg(lower_color, &mut output, &mut current_fg);
                '▄'
            } else if combined >= BLOCK_THRESHOLD {
                set_fg(avg_color, &mut output, &mut current_fg);
                set_bg(avg_color, &mut output, &mut current_bg);
                shade_glyph(combined)
            } else {
                set_fg(avg_color, &mut output, &mut current_fg);
                set_bg(avg_color, &mut output, &mut current_bg);
                '░'
            };
            output.push(glyph);
        }
        reset_attrs(&mut output, &mut current_fg, &mut current_bg);
        output.push('\n');
    }
    Ok(output)
}

fn shade_glyph(intensity: f32) -> char {
    if intensity >= 0.75 {
        '█'
    } else if intensity >= 0.5 {
        '▓'
    } else if intensity >= 0.3 {
        '▒'
    } else {
        '░'
    }
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
    use image::{DynamicImage, ImageBuffer, Rgb};
    use std::path::Path;

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

    #[test]
    fn block_converter_respects_dimensions() {
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(8, 8, |x, y| {
            if (x + y) % 2 == 0 {
                Rgb([200, 200, 200])
            } else {
                Rgb([50, 50, 50])
            }
        }));
        let width = 6;
        let height = 5;
        let art = convert_image_to_blocks(&img, width, Some(height), DEFAULT_CELL_ASPECT).unwrap();
        let lines: Vec<&str> = art.lines().collect();
        assert_eq!(lines.len(), height as usize);
        for line in lines {
            let stripped = strip_ansi(line);
            assert_eq!(stripped.chars().count(), width as usize);
        }
    }

    #[test]
    fn block_converter_handles_demo_image() {
        let path = Path::new("../../assets/demo.png");
        if path.exists() {
            let img = image::open(path).expect("demo image");
            let width = 20;
            let height = 12;
            let art =
                convert_image_to_blocks(&img, width, Some(height), DEFAULT_CELL_ASPECT).unwrap();
            let lines: Vec<&str> = art.lines().collect();
            assert_eq!(lines.len(), height as usize);
            for line in lines {
                let stripped = strip_ansi(line);
                assert_eq!(stripped.chars().count(), width as usize);
            }
        }
    }

    fn strip_ansi(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                while let Some(c) = chars.next() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }
}
