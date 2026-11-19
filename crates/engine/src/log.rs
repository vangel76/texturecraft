//! The session log: every entry has a source, a level and a message. The Log panel shows
//! `[Source] message` coloured by level, and Help > Export log... writes the level as text.

use std::ops::Deref;

/// How serious an entry is
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogLevel {
    #[default]
    Info,
    Warning,
    Error,
}

impl LogLevel {
    /// The level as the upper-case tag of the exported log file
    pub fn tag(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// The source of entries that came without one (plain `push`)
pub const DEFAULT_SOURCE: &str = "TextureCraft";

/// The source of everything about resources, templates, presets and materials: the Log
/// panel's Assets tab shows these entries
pub const LIBRARY_SOURCE: &str = "Library";

/// One log line
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub source: String,
    pub level: LogLevel,
    pub message: String,
}

impl LogEntry {
    pub fn new(level: LogLevel, source: &str, message: impl Into<String>) -> LogEntry {
        LogEntry { source: source.to_string(), level, message: message.into() }
    }

    /// The panel's line: `[Source] message`, no time stamp and no level tag
    pub fn line(&self) -> String {
        format!("[{}] {}", self.source, self.message)
    }

    /// True for entries of the Log panel's Assets tab
    pub fn is_asset(&self) -> bool {
        self.source == LIBRARY_SOURCE
    }
}

/// The message text, so `entry.contains(..)` and `entry.starts_with(..)` read the message
impl Deref for LogEntry {
    type Target = str;
    fn deref(&self) -> &str {
        &self.message
    }
}

impl PartialEq<String> for LogEntry {
    fn eq(&self, other: &String) -> bool {
        self.message == *other
    }
}

impl PartialEq<str> for LogEntry {
    fn eq(&self, other: &str) -> bool {
        self.message == other
    }
}

/// The entries of a session, oldest first
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogStore(Vec<LogEntry>);

impl LogStore {
    pub fn new() -> LogStore {
        LogStore(Vec::new())
    }

    /// An info entry without a source of its own
    pub fn push(&mut self, message: String) {
        self.0.push(LogEntry::new(LogLevel::Info, DEFAULT_SOURCE, message));
    }

    pub fn info(&mut self, source: &str, message: impl Into<String>) {
        self.0.push(LogEntry::new(LogLevel::Info, source, message));
    }

    pub fn warn(&mut self, source: &str, message: impl Into<String>) {
        self.0.push(LogEntry::new(LogLevel::Warning, source, message));
    }

    pub fn error(&mut self, source: &str, message: impl Into<String>) {
        self.0.push(LogEntry::new(LogLevel::Error, source, message));
    }

    /// Several warnings of one source
    pub fn warnings(&mut self, source: &str, messages: impl IntoIterator<Item = String>) {
        for message in messages {
            self.warn(source, message);
        }
    }

    /// Append the entries of another store
    pub fn append(&mut self, other: LogStore) {
        self.0.extend(other.0);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Drop the entries for which `keep` is false
    pub fn retain(&mut self, keep: impl FnMut(&LogEntry) -> bool) {
        self.0.retain(keep);
    }
}

impl Deref for LogStore {
    type Target = [LogEntry];
    fn deref(&self) -> &[LogEntry] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a LogStore {
    type Item = &'a LogEntry;
    type IntoIter = std::slice::Iter<'a, LogEntry>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_keep_source_level_and_message() {
        let mut log = LogStore::new();
        log.warn("Exporter", "map missing");
        log.error("Library", "cannot be deleted");
        log.push("plain".into());
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].line(), "[Exporter] map missing");
        assert_eq!(log[0].level, LogLevel::Warning);
        assert_eq!(log[1].level, LogLevel::Error);
        assert!(log[1].is_asset());
        assert_eq!(log[2].level, LogLevel::Info);
        assert!(log[2].starts_with("pl"));
    }

    #[test]
    fn retain_and_clear() {
        let mut log = LogStore::new();
        log.info("A", "a");
        log.info(LIBRARY_SOURCE, "b");
        log.retain(|entry| !entry.is_asset());
        assert_eq!(log.len(), 1);
        log.clear();
        assert!(log.is_empty());
    }
}
