use std::path::PathBuf;

use anyhow::Result;
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
