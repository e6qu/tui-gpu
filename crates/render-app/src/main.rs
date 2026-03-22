use anyhow::Result;
use clap::{Parser, ValueEnum};
use renderer_core::{run_app_with_options, AppMode, ComputeMode, DemoKind, RendererOptions};
use std::time::Duration;

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, value_enum, default_value_t = CliDemo::Terminal)]
    demo: CliDemo,
    #[arg(long, value_enum, default_value_t = CliMode::Gui)]
    mode: CliMode,
    #[arg(long, value_enum, default_value_t = CliCompute::Cpu)]
    compute: CliCompute,
    /// Log average FPS every N seconds (0 disables logging)
    #[arg(long, default_value_t = 0.0)]
    fps_log: f32,
    /// Enable or disable audio for the YouTube demo
    #[arg(long, default_value = "true")]
    youtube_audio: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliDemo {
    Terminal,
    Plasma,
    Ray,
    Doom,
    Youtube,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliMode {
    Gui,
    Tui,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliCompute {
    Cpu,
    Gpu,
}

impl From<CliDemo> for DemoKind {
    fn from(value: CliDemo) -> Self {
        match value {
            CliDemo::Terminal => DemoKind::Terminal,
            CliDemo::Plasma => DemoKind::Plasma,
            CliDemo::Ray => DemoKind::Ray,
            CliDemo::Doom => DemoKind::Doom,
            CliDemo::Youtube => DemoKind::Youtube,
        }
    }
}

impl From<CliMode> for AppMode {
    fn from(value: CliMode) -> Self {
        match value {
            CliMode::Gui => AppMode::Gui,
            CliMode::Tui => AppMode::Tui,
        }
    }
}

impl From<CliCompute> for ComputeMode {
    fn from(value: CliCompute) -> Self {
        match value {
            CliCompute::Cpu => ComputeMode::Cpu,
            CliCompute::Gpu => ComputeMode::Gpu,
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let fps_interval = if cli.fps_log > 0.0 {
        Some(Duration::from_secs_f32(cli.fps_log))
    } else {
        None
    };
    let options = RendererOptions {
        fps_sample_interval: fps_interval,
        youtube_audio: Some(cli.youtube_audio),
    };
    run_app_with_options(
        cli.demo.into(),
        cli.compute.into(),
        cli.mode.into(),
        options,
    )
}
