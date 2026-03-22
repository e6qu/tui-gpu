use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type EventId = String;
pub type AgentId = String;
pub type ChannelId = String;
pub type SessionId = String;
pub type TopicLabel = String;
pub type ActorId = String;

/// Canonical event record stored in the append-only log.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Event {
    pub id: EventId,
    pub parents: Vec<EventId>,
    pub timestamp: DateTime<Utc>,
    pub actor: ActorId,
    pub payload: EventPayload,
    pub schema_version: u16,
    pub signature: Option<String>,
}

impl Event {
    pub fn new(actor: impl Into<ActorId>, payload: EventPayload) -> Self {
        let timestamp = Utc::now();
        let id = format!("evt_{}", timestamp.timestamp_nanos_opt().unwrap_or(0).abs());
        Self {
            id,
            parents: Vec::new(),
            timestamp,
            actor: actor.into(),
            payload,
            schema_version: 1,
            signature: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum EventPayload {
    AgentMessage {
        message_id: String,
        sent_at: DateTime<Utc>,
        channel: ChannelId,
        author: AgentId,
        session: SessionId,
        content: String,
        labels: Vec<TopicLabel>,
    },
    ModeChanged {
        agent: AgentId,
        from: String,
        to: String,
    },
    SignalRaised {
        agent: AgentId,
        kind: String,
        severity: Severity,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// Append-only event log writer.
pub struct EventLogWriter {
    root: PathBuf,
}

impl EventLogWriter {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn append(&self, event: &Event) -> Result<()> {
        let day = event.timestamp.format("%Y-%m-%d").to_string();
        let dir = self.root.join("events");
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(format!("{day}.log"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        let line = serde_json::to_vec(event)?;
        writer.write_all(&line)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }
}

/// Simple content-addressed store used for conversation graph nodes.
pub struct CasStore {
    root: PathBuf,
}

impl CasStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        if !root.exists() {
            fs::create_dir_all(&root)
                .with_context(|| format!("creating CAS root {}", root.display()))?;
        }
        Ok(Self { root })
    }

    pub fn put(&self, namespace: &str, data: &[u8]) -> Result<String> {
        if namespace.is_empty() {
            bail!("namespace cannot be empty");
        }
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = hasher.finalize();
        let hash = hex::encode(digest);
        let (dir, file) = hash.split_at(2);
        let object_dir = self.root.join(namespace).join(dir);
        fs::create_dir_all(&object_dir)
            .with_context(|| format!("creating {}", object_dir.display()))?;
        let path = object_dir.join(file);
        if !path.exists() {
            fs::write(&path, data)
                .with_context(|| format!("writing CAS object {}", path.display()))?;
        }
        Ok(hash)
    }

    pub fn get(&self, namespace: &str, hash: &str) -> Result<Vec<u8>> {
        if hash.len() < 3 {
            bail!("invalid hash {hash}");
        }
        let (dir, file) = hash.split_at(2);
        let path = self.root.join(namespace).join(dir).join(file);
        let data = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cas_round_trip() {
        let dir = tempdir().unwrap();
        let store = CasStore::new(dir.path()).unwrap();
        let hash = store.put("graph", b"hello world").unwrap();
        let data = store.get("graph", &hash).unwrap();
        assert_eq!(data, b"hello world");
    }

    #[test]
    fn log_writer_appends() {
        let dir = tempdir().unwrap();
        let writer = EventLogWriter::new(dir.path());
        let event = Event {
            id: "evt_test".into(),
            parents: vec![],
            timestamp: Utc::now(),
            actor: "agent_a".into(),
            payload: EventPayload::ModeChanged {
                agent: "agent_a".into(),
                from: "plan".into(),
                to: "build".into(),
            },
            schema_version: 1,
            signature: None,
        };
        writer.append(&event).unwrap();
        let day = event.timestamp.format("%Y-%m-%d").to_string();
        let path = dir.path().join("events").join(format!("{day}.log"));
        let data = fs::read_to_string(path).unwrap();
        assert!(data.contains("evt_test"));
    }
}
