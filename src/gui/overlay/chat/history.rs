//! `ChatComponent.recentChat` and vanilla's `CommandHistory`.

use std::collections::VecDeque;
use std::path::PathBuf;

/// `ChatComponent.MAX_CHAT_HISTORY`: sent messages remembered.
const MAX_RECENT: usize = 100;
/// `CommandHistory.MAX_PERSISTED_COMMAND_HISTORY`.
const MAX_PERSISTED_COMMANDS: usize = 50;

pub struct CommandHistory {
    /// Sent messages and commands, oldest first (`recentChat`).
    recent: VecDeque<String>,
    /// `command_history.txt`, if there is a game directory.
    file: Option<PathBuf>,
    /// The commands kept in the file.
    persisted: VecDeque<String>,
}

impl CommandHistory {
    /// Reads `command_history.txt` from `game_dir`.
    pub fn load(game_dir: Option<PathBuf>) -> Self {
        let file = game_dir.map(|dir| dir.join("command_history.txt"));
        let persisted: VecDeque<String> = file
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        Self {
            recent: persisted.clone(),
            file,
            persisted,
        }
    }

    pub fn len(&self) -> usize {
        self.recent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recent.is_empty()
    }

    /// Entry `index`, oldest first.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.recent.get(index).map(String::as_str)
    }

    /// `addRecentChat`.
    pub fn add(&mut self, message: &str) {
        if self.recent.back().map(String::as_str) != Some(message) {
            if self.recent.len() >= MAX_RECENT {
                self.recent.pop_front();
            }
            self.recent.push_back(message.to_owned());
        }
        if !message.starts_with('/') {
            return;
        }
        if self.persisted.back().map(String::as_str) != Some(message) {
            self.persisted.push_back(message.to_owned());
            while self.persisted.len() > MAX_PERSISTED_COMMANDS {
                self.persisted.pop_front();
            }
        }
        if let Some(path) = &self.file {
            let mut text = self
                .persisted
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n");
            text.push('\n');
            let _ = std::fs::write(path, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_are_kept_once_and_only_commands_persist() {
        let dir = std::env::temp_dir().join(format!("melon-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut history = CommandHistory::load(Some(dir.clone()));
        history.add("hello");
        history.add("/time set day");
        history.add("/time set day");
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(1), Some("/time set day"));
        let reloaded = CommandHistory::load(Some(dir.clone()));
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.get(0), Some("/time set day"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
