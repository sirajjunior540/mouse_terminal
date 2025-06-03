use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use chrono::Local;

/// Default maximum number of history entries to keep
const DEFAULT_MAX_HISTORY: usize = 500;

/// Command history manager
#[derive(Debug, Serialize, Deserialize)]
pub struct History {
    /// The command history
    pub commands: VecDeque<String>,
    /// Maximum number of history entries to keep
    max_history: usize,
    /// Current position when navigating history
    #[serde(skip)]
    current_position: Option<usize>,
    /// Path to the history file
    #[serde(skip)]
    history_file: Option<PathBuf>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            commands: VecDeque::new(),
            max_history: DEFAULT_MAX_HISTORY,
            current_position: None,
            history_file: None,
        }
    }
}

impl History {
    /// Create a new history manager with default settings
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new history manager with custom settings
    #[allow(dead_code)]
    pub fn with_max_history(max_history: usize) -> Self {
        Self {
            max_history,
            ..Self::default()
        }
    }

    /// Add a command to the history
    pub fn add(&mut self, command: String) {
        // Don't add empty commands or duplicates of the most recent command
        if command.trim().is_empty() || self.commands.front().map_or(false, |c| c == &command) {
            return;
        }

        // Add the command to the front
        self.commands.push_front(command);

        // Trim history if it exceeds the maximum size
        while self.commands.len() > self.max_history {
            self.commands.pop_back();
        }

        // Reset the current position
        self.current_position = None;
    }

    /// Get the previous command in history (moving backward)
    pub fn previous(&mut self) -> Option<&String> {
        if self.commands.is_empty() {
            return None;
        }

        let new_pos = match self.current_position {
            None => 0,
            Some(pos) if pos + 1 < self.commands.len() => pos + 1,
            Some(pos) => pos,
        };

        self.current_position = Some(new_pos);
        self.commands.get(new_pos)
    }

    /// Get the next command in history (moving forward)
    pub fn next(&mut self) -> Option<&String> {
        match self.current_position {
            Some(0) => {
                self.current_position = None;
                None
            }
            Some(pos) => {
                let new_pos = pos - 1;
                self.current_position = Some(new_pos);
                self.commands.get(new_pos)
            }
            None => None,
        }
    }

    /// Reset the history navigation position
    #[allow(dead_code)]
    pub fn reset_position(&mut self) {
        self.current_position = None;
    }

    /// Load history from the default location
    pub fn load_default() -> Result<Self> {
        let mut history = Self::default();
        if let Some(history_path) = Self::default_history_path()? {
            history.history_file = Some(history_path.clone());

            // Create the directory if it doesn't exist
            if let Some(parent) = history_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Try to load the history file
            match File::open(&history_path) {
                Ok(mut file) => {
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)?;

                    // Parse the JSON
                    let loaded: Self = serde_json::from_str(&contents)?;
                    history.commands = loaded.commands;
                    history.max_history = loaded.max_history;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // File doesn't exist yet, that's fine
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(history)
    }

    /// Save history to the default location
    pub fn save(&self) -> Result<()> {
        let default_path = Self::default_history_path().ok().flatten();
        if let Some(history_path) = self.history_file.as_ref().or(default_path.as_ref()) {
            // Create the directory if it doesn't exist
            if let Some(parent) = history_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Create a backup before saving
            self.create_backup()?;

            // Serialize to JSON
            let json = serde_json::to_string(self)?;

            // Write to file
            let mut file = File::create(history_path)?;
            file.write_all(json.as_bytes())?;
        }

        Ok(())
    }

    /// Create a backup of the history file
    pub fn create_backup(&self) -> Result<()> {
        let default_path = Self::default_history_path().ok().flatten();
        if let Some(history_path) = self.history_file.as_ref().or(default_path.as_ref()) {
            // Check if the history file exists
            if !history_path.exists() {
                return Ok(());
            }

            // Create the backup directory
            let backup_dir = dirs::home_dir()
                .map(|home| home.join(".mouse_term").join("history_backups"))
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

            fs::create_dir_all(&backup_dir)?;

            // Generate a timestamp for the backup file
            let now = Local::now();
            let timestamp = now.format("%Y%m%d_%H%M%S").to_string();

            // Create the backup file path
            let backup_path = backup_dir.join(format!("history_{}.json", timestamp));

            // Copy the history file to the backup file
            fs::copy(history_path, backup_path)?;
        }

        Ok(())
    }

    /// Get the default history file path
    fn default_history_path() -> Result<Option<PathBuf>> {
        Ok(dirs::home_dir().map(|home| home.join(".mouse_term").join("history.json")))
    }

    /// Set the maximum history size
    #[allow(dead_code)]
    pub fn set_max_history(&mut self, max_history: usize) {
        self.max_history = max_history;

        // Trim history if it exceeds the new maximum size
        while self.commands.len() > self.max_history {
            self.commands.pop_back();
        }
    }

    /// Get a specific command by index
    pub fn get(&self, index: usize) -> Option<&String> {
        self.commands.get(index)
    }

    /// Get the number of commands in history
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if history is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Search for commands in history that contain the given query
    #[allow(dead_code)]
    pub fn search(&self, query: &str) -> Vec<String> {
        let query = query.to_lowercase();
        self.commands
            .iter()
            .filter(|cmd| cmd.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    /// Get the path to the backup directory
    #[allow(dead_code)]
    pub fn backup_dir() -> Result<PathBuf> {
        dirs::home_dir()
            .map(|home| home.join(".mouse_term").join("history_backups"))
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))
    }

    /// List all available backups
    #[allow(dead_code)]
    pub fn list_backups() -> Result<Vec<PathBuf>> {
        let backup_dir = Self::backup_dir()?;

        if !backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();
        for entry in fs::read_dir(backup_dir)? {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    backups.push(path);
                }
            }
        }

        // Sort backups by modification time (newest first)
        backups.sort_by(|a, b| {
            let a_time = fs::metadata(a).and_then(|m| m.modified()).ok();
            let b_time = fs::metadata(b).and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        Ok(backups)
    }

    /// Restore history from a backup file
    #[allow(dead_code)]
    pub fn restore_from_backup(backup_path: &PathBuf) -> Result<Self> {
        let mut file = File::open(backup_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // Parse the JSON
        let mut history: Self = serde_json::from_str(&contents)?;

        // Set the history file path to the default
        history.history_file = Self::default_history_path().ok().flatten();

        Ok(history)
    }
}
