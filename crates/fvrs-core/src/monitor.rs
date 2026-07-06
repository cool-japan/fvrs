//! File system monitoring: watcher lifecycle, events, filters, settings and history

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufReader as StdBufReader, BufWriter};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::error::TrySendError;

use crate::error::{FsError, FsResult};
use crate::fs::FileSystem;

/// Capacity of the bounded watcher event channel.
///
/// When the receiver falls behind and the buffer fills up, further events are
/// dropped (with a `tracing::warn!`) instead of growing memory without bound.
const EVENT_CHANNEL_CAPACITY: usize = 4096;

/// Forward a notify event into the bounded channel, dropping it with a
/// warning when the receiver has fallen behind (channel full).
fn forward_event(tx: &tokio::sync::mpsc::Sender<FsEvent>, event: FsEvent) {
    match tx.try_send(event) {
        Ok(()) => {}
        Err(TrySendError::Full(dropped)) => {
            tracing::warn!(
                path = %dropped.path.display(),
                capacity = EVENT_CHANNEL_CAPACITY,
                "file system event dropped: watcher channel full"
            );
        }
        // Receiver already gone (stop_watching raced the callback) — nothing to do.
        Err(TrySendError::Closed(_)) => {}
    }
}

/// Event type for file system monitoring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FsEventType {
    /// File or directory created
    Create,
    /// File or directory modified
    Modify,
    /// File or directory removed
    Remove,
    /// File or directory renamed
    Rename,
    /// File or directory accessed
    Access,
    /// File or directory metadata changed
    Metadata,
}

/// File system event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEvent {
    /// Event type
    pub event_type: FsEventType,
    /// Path to the file or directory
    pub path: PathBuf,
    /// Timestamp of the event
    pub timestamp: DateTime<Local>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl From<Event> for FsEvent {
    fn from(event: Event) -> Self {
        let event_type = match event.kind {
            EventKind::Create(_) => FsEventType::Create,
            EventKind::Modify(_) => FsEventType::Modify,
            EventKind::Remove(_) => FsEventType::Remove,
            EventKind::Access(_) => FsEventType::Access,
            _ => FsEventType::Metadata,
        };
        let path = event.paths.first().cloned().unwrap_or_default();
        let timestamp = Local::now();
        let mut metadata = HashMap::new();
        match event.kind {
            EventKind::Modify(ModifyKind::Data(_)) => {
                metadata.insert("modification_type".to_string(), "data".to_string());
            }
            EventKind::Modify(ModifyKind::Metadata(_)) => {
                metadata.insert("modification_type".to_string(), "metadata".to_string());
            }
            _ => {}
        }
        Self {
            event_type,
            path,
            timestamp,
            metadata,
        }
    }
}

/// Monitoring filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringFilter {
    /// File patterns to include (glob)
    pub include_patterns: Vec<String>,
    /// File patterns to exclude (glob)
    pub exclude_patterns: Vec<String>,
    /// Event types to monitor
    pub event_types: HashSet<FsEventType>,
    /// Minimum file size to monitor
    pub min_size: Option<u64>,
    /// Maximum file size to monitor
    pub max_size: Option<u64>,
    /// File extensions to monitor
    pub extensions: HashSet<String>,
}

impl MonitoringFilter {
    /// Create new monitoring filter
    pub fn new() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            event_types: HashSet::new(),
            min_size: None,
            max_size: None,
            extensions: HashSet::new(),
        }
    }

    /// Check if path matches filter
    pub fn matches(&self, path: &Path) -> bool {
        // Check include patterns
        if !self.include_patterns.is_empty() {
            let matches_include = self.include_patterns.iter().any(|pattern| {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(path.to_str().unwrap_or("")))
                    .unwrap_or(false)
            });
            if !matches_include {
                return false;
            }
        }

        // Check exclude patterns
        if self.exclude_patterns.iter().any(|pattern| {
            glob::Pattern::new(pattern)
                .map(|p| p.matches(path.to_str().unwrap_or("")))
                .unwrap_or(false)
        }) {
            return false;
        }

        // Check extensions
        if !self.extensions.is_empty() {
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    if !self.extensions.contains(ext_str) {
                        return false;
                    }
                }
            }
        }

        true
    }
}

impl Default for MonitoringFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitoring settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSettings {
    /// Path to monitor
    pub path: PathBuf,
    /// Whether to monitor recursively
    pub recursive: bool,
    /// Filter settings
    pub filter: MonitoringFilter,
    /// Maximum number of events to keep in history
    pub max_history: usize,
    /// Debounce time in milliseconds
    pub debounce_ms: u64,
}

/// Monitoring history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringHistory {
    /// List of events
    pub events: VecDeque<FsEvent>,
    /// Maximum number of events to keep
    pub max_events: usize,
}

impl MonitoringHistory {
    /// Create new monitoring history
    pub fn new(max_events: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_events),
            max_events,
        }
    }

    /// Add event to history
    pub fn add_event(&mut self, event: FsEvent) {
        if self.events.len() >= self.max_events {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Get events within time range
    pub fn get_events_in_range(
        &self,
        start: DateTime<Local>,
        end: DateTime<Local>,
    ) -> Vec<&FsEvent> {
        self.events
            .iter()
            .filter(|event| event.timestamp >= start && event.timestamp <= end)
            .collect()
    }

    /// Get events by type
    pub fn get_events_by_type(&self, event_type: &FsEventType) -> Vec<&FsEvent> {
        self.events
            .iter()
            .filter(|event| event.event_type == *event_type)
            .collect()
    }

    /// Save history to file
    pub fn save_to_file(&self, path: &PathBuf) -> FsResult<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.events)
            .map_err(|e| FsError::Serialization(e.to_string()))?;
        Ok(())
    }

    /// Load history from file
    pub fn load_from_file(path: &PathBuf, max_events: usize) -> FsResult<Self> {
        let file = File::open(path)?;
        let reader = StdBufReader::new(file);
        let events: Vec<FsEvent> =
            serde_json::from_reader(reader).map_err(|e| FsError::Serialization(e.to_string()))?;

        let mut history = Self::new(max_events);
        for event in events {
            history.add_event(event);
        }
        Ok(history)
    }
}

impl FileSystem {
    /// Start watching for file system events
    ///
    /// The watcher is stored inside this `FileSystem` and stays alive until
    /// [`FileSystem::stop_watching`] is called or a new watch replaces it.
    pub async fn watch_directory(&mut self, path: &Path) -> FsResult<()> {
        let (tx, rx) = channel(EVENT_CHANNEL_CAPACITY);
        let mut watcher =
            notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if !event.paths.is_empty() {
                        forward_event(&tx, FsEvent::from(event));
                    }
                }
            })?;
        watcher.watch(path, RecursiveMode::Recursive)?;
        self.event_receiver = Some(rx);
        self.watcher = Some(watcher);
        Ok(())
    }

    /// Await the next file system event
    ///
    /// Returns `None` if no watch is active or the watcher has been dropped.
    pub async fn next_event(&mut self) -> Option<FsEvent> {
        match self.event_receiver.as_mut() {
            Some(rx) => rx.recv().await,
            None => None,
        }
    }

    /// Poll for a pending file system event without blocking
    pub fn try_next_event(&mut self) -> Option<FsEvent> {
        self.event_receiver
            .as_mut()
            .and_then(|rx| rx.try_recv().ok())
    }

    /// Stop watching for file system events and release the watcher
    pub fn stop_watching(&mut self) {
        self.watcher = None;
        self.event_receiver = None;
    }

    /// Start monitoring with settings
    ///
    /// The watcher is stored inside this `FileSystem` and stays alive until
    /// [`FileSystem::stop_watching`] is called or a new watch replaces it.
    pub async fn start_monitoring_with_settings(
        &mut self,
        settings: MonitoringSettings,
    ) -> FsResult<()> {
        let (tx, rx) = channel(EVENT_CHANNEL_CAPACITY);
        let settings_cloned = settings.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let matches = event
                        .paths
                        .first()
                        .map(|path| settings_cloned.filter.matches(path))
                        .unwrap_or(false);
                    if matches {
                        forward_event(&tx, FsEvent::from(event));
                    }
                }
            })?;
        watcher.watch(
            &settings.path,
            if settings.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            },
        )?;
        self.event_receiver = Some(rx);
        self.watcher = Some(watcher);
        Ok(())
    }

    /// Save monitoring settings to file
    pub async fn save_monitoring_settings(
        &self,
        settings: &MonitoringSettings,
        path: &PathBuf,
    ) -> FsResult<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, settings)
            .map_err(|e| FsError::Serialization(e.to_string()))?;
        Ok(())
    }

    /// Load monitoring settings from file
    pub async fn load_monitoring_settings(&self, path: &PathBuf) -> FsResult<MonitoringSettings> {
        let file = File::open(path)?;
        let reader = StdBufReader::new(file);
        let settings: MonitoringSettings =
            serde_json::from_reader(reader).map_err(|e| FsError::Serialization(e.to_string()))?;
        Ok(settings)
    }
}
