use std::path::PathBuf;

use ansi_image::{convert_image_to_ansi, DEFAULT_CELL_ASPECT, DEFAULT_PALETTE};
use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use runtime_core::{Event, EventLogReader, EventLogWriter, EventPayload};
use serde_json;

#[derive(Parser)]
#[command(author, version, about = "Runtime utility for event log operations")]
struct Cli {
    /// Root directory for event/cas storage
    #[arg(long, default_value = "data/runtime")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Append an agent message event
    AppendMessage {
        #[arg(long)]
        actor: String,
        #[arg(long)]
        channel: String,
        #[arg(long)]
        session: String,
        #[arg(long)]
        content: String,
        #[arg(long, value_delimiter = ',')]
        labels: Vec<String>,
    },
    /// List events for a given day (YYYY-MM-DD)
    List {
        #[arg(long)]
        day: Option<String>,
    },
    /// Convert an image into ANSI-colored ASCII art for the TUI
    ImageToAnsi {
        #[arg(long)]
        input: PathBuf,
        /// Target width in terminal cells/columns
        #[arg(long, default_value_t = 80)]
        width: u32,
        /// Optional target height in rows; otherwise auto-computed from aspect ratio
        #[arg(long)]
        height: Option<u32>,
        /// Characters ordered from lightest to darkest to use for ramp
        #[arg(long, default_value = DEFAULT_PALETTE)]
        palette: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::AppendMessage {
            actor,
            channel,
            session,
            content,
            labels,
        } => append_message(&cli.root, actor, channel, session, content, labels),
        Commands::List { day } => list_events(&cli.root, day),
        Commands::ImageToAnsi {
            input,
            width,
            height,
            palette,
        } => image_to_ansi(&input, width, height, &palette),
    }
}

fn append_message(
    root: &PathBuf,
    actor: String,
    channel: String,
    session: String,
    content: String,
    labels: Vec<String>,
) -> Result<()> {
    let writer = EventLogWriter::new(root);
    let event = Event::new(
        actor.clone(),
        EventPayload::AgentMessage {
            message_id: format!("msg_{}", Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            sent_at: Utc::now(),
            channel,
            author: actor,
            session,
            content,
            labels,
        },
    );
    writer.append(&event)?;
    println!("Appended event {}", event.id);
    Ok(())
}

fn list_events(root: &PathBuf, day: Option<String>) -> Result<()> {
    let reader = EventLogReader::new(root);
    let day = day.unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
    let events = reader.read_day(&day)?;
    if events.is_empty() {
        println!("No events for {day}");
    } else {
        for event in events {
            let payload = serde_json::to_string(&event.payload)?;
            println!("{} {}", event.timestamp, payload);
        }
    }
    Ok(())
}

fn image_to_ansi(input: &PathBuf, width: u32, height: Option<u32>, palette: &str) -> Result<()> {
    let image = image::io::Reader::open(input)
        .with_context(|| format!("opening image {}", input.display()))?
        .decode()
        .with_context(|| format!("decoding image {}", input.display()))?;
    let chars: Vec<char> = palette.chars().collect();
    let art = convert_image_to_ansi(&image, width, height, &chars, DEFAULT_CELL_ASPECT)?;
    print!("{art}\x1b[0m");
    Ok(())
}
