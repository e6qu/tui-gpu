use std::{fs, path::PathBuf};

use ansi_image::{convert_image_to_ansi, DEFAULT_CELL_ASPECT, DEFAULT_PALETTE};
use anyhow::Result;
use clap::Parser;
use image::DynamicImage;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value_t = 80)]
    width: u32,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long, default_value = DEFAULT_PALETTE)]
    palette: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let bytes = fs::read(&cli.input)?;
    let img = image::load_from_memory(&bytes)?;
    let palette: Vec<char> = cli.palette.chars().collect();
    let ansi = convert_image_to_ansi(&img, cli.width, cli.height, &palette, DEFAULT_CELL_ASPECT)?;
    print!("{}", ansi);
    Ok(())
}
