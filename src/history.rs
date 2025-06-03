use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

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
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a new history manager with custom settings
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
        if let Some(history_path) = self.history_file.as_ref().or_else(|| Self::default_history_path().ok().flatten()) {
            // Create the directory if it doesn't exist
            if let Some(parent) = history_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Serialize to JSON
            let json = serde_json::to_string(self)?;
            
            // Write to file
            let mut file = File::create(history_path)?;
            file.write_all(json.as_bytes())?;
        }
        
        Ok(())
    }
    
    /// Get the default history file path
    fn default_history_path() -> Result<Option<PathBuf>> {
        Ok(dirs::home_dir().map(|home| home.join(".mouse_term").join("history.json")))
    }
    
    /// Set the maximum history size
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
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}