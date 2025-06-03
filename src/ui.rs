use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;
use std::path::{Path, PathBuf};
use std::fs;

use crate::input::InputState;
use crate::history::History;

/// UI state for the application
pub struct UiState {
    /// Whether the history sidebar is visible
    pub show_history: bool,
    /// Whether a command is currently running
    pub is_running: bool,
    /// Current output from command execution
    pub output: Vec<String>,
    /// Current hover position (token index)
    pub hover_token: Option<usize>,
    /// Currently editing token index
    pub editing_token: Option<usize>,
    /// Files and folders in the current directory
    pub files: Vec<FileInfo>,
    /// Current working directory
    pub current_dir: PathBuf,
    /// Hover position in the file list
    pub hover_file: Option<usize>,
    /// Whether we're waiting for a sudo password
    pub sudo_password_prompt: bool,
    /// The sudo password being entered
    pub sudo_password: String,
    /// The command that needs sudo
    pub sudo_command: Option<String>,
    /// Spinner frame for loading animation
    pub spinner_frame: usize,
    /// Last update time for spinner
    pub last_spinner_update: std::time::Instant,
}

/// Information about a file or folder
pub struct FileInfo {
    /// Name of the file or folder
    pub name: String,
    /// Whether it's a directory
    pub is_dir: bool,
    /// File size (if it's a file)
    pub size: Option<u64>,
    /// Last modified time
    pub modified: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_history: false,
            is_running: false,
            output: Vec::new(),
            hover_token: None,
            editing_token: None,
            files: Vec::new(),
            current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            hover_file: None,
            sudo_password_prompt: false,
            sudo_password: String::new(),
            sudo_command: None,
            spinner_frame: 0,
            last_spinner_update: std::time::Instant::now(),
        }
    }
}

impl FileInfo {
    /// Create a new FileInfo from a path
    pub fn from_path(path: &Path) -> Self {
        let metadata = fs::metadata(path).ok();
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = metadata.as_ref().map(|m| m.len());
        let modified = metadata.and_then(|m| m.modified().ok())
            .map(|time| {
                let datetime = chrono::DateTime::<chrono::Local>::from(time);
                datetime.format("%Y-%m-%d %H:%M").to_string()
            });

        Self {
            name,
            is_dir,
            size,
            modified,
        }
    }

    /// Get the icon for this file or folder
    pub fn get_icon(&self) -> &'static str {
        if self.is_dir {
            "📁 "
        } else {
            match self.name.rsplit('.').next() {
                Some("txt") | Some("md") | Some("rs") | Some("toml") => "📄 ",
                Some("jpg") | Some("png") | Some("gif") => "🖼️ ",
                Some("mp3") | Some("wav") | Some("ogg") => "🎵 ",
                Some("mp4") | Some("avi") | Some("mkv") => "🎬 ",
                Some("zip") | Some("tar") | Some("gz") => "📦 ",
                Some("exe") | Some("sh") | Some("bat") => "🛠️ ",
                _ => "📄 ",
            }
        }
    }
}

/// Renders the entire UI
pub fn render(frame: &mut Frame, ui_state: &mut UiState, input_state: &InputState, history: &History) {
    let size = frame.size();

    // Create the main layout (vertical split)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(83), // Main viewport
            Constraint::Length(2),      // Status bar
            Constraint::Percentage(15), // Input line
        ])
        .split(size);

    // If history sidebar is enabled, create a horizontal split for the main area
    let (main_area, input_area) = if ui_state.show_history {
        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(70), // Main viewport
                Constraint::Percentage(30), // History sidebar
            ])
            .split(chunks[0]);

        render_history(frame, horizontal_chunks[1], history);
        (horizontal_chunks[0], chunks[2])
    } else {
        (chunks[0], chunks[2])
    };

    render_output(frame, main_area, ui_state);
    render_status_bar(frame, chunks[1], ui_state);
    render_input(frame, input_area, input_state, ui_state);

    // If we're waiting for a sudo password, render the password prompt
    if ui_state.sudo_password_prompt {
        render_sudo_password_prompt(frame, size, ui_state);
    }

    // Update spinner frame if command is running
    if ui_state.is_running {
        let now = std::time::Instant::now();
        if now.duration_since(ui_state.last_spinner_update).as_millis() > 100 {
            ui_state.spinner_frame = (ui_state.spinner_frame + 1) % 8;
            ui_state.last_spinner_update = now;
        }
    }
}

/// Renders the output viewport
fn render_output(frame: &mut Frame, area: Rect, ui_state: &UiState) {
    // Split the area into two parts: output and file list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60), // Command output
            Constraint::Percentage(40), // File list
        ])
        .split(area);

    // Render command output
    let output_text: Vec<String> = ui_state.output
        .iter()
        .map(|line| line.clone())
        .collect();

    let output_widget = Paragraph::new(output_text.join("\n"))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" 📺 Output - {} ", ui_state.current_dir.display()))
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .wrap(Wrap { trim: true });

    frame.render_widget(output_widget, chunks[0]);

    // Render file list
    render_file_list(frame, chunks[1], ui_state);
}

/// Renders the file list
fn render_file_list(frame: &mut Frame, area: Rect, ui_state: &UiState) {
    let mut items = Vec::new();

    // Create a list item for each file/folder
    for (idx, file) in ui_state.files.iter().enumerate() {
        let icon = file.get_icon();
        let name = &file.name;

        // Format size
        let size_str = match file.size {
            Some(size) if size < 1024 => format!("{}B", size),
            Some(size) if size < 1024 * 1024 => format!("{:.1}KB", size as f64 / 1024.0),
            Some(size) => format!("{:.1}MB", size as f64 / (1024.0 * 1024.0)),
            None => "".to_string(),
        };

        // Format modified time
        let modified_str = file.modified.as_deref().unwrap_or("");

        // Create the display text
        let display_text = format!("{}{} {} {}", 
            icon, 
            name,
            if file.is_dir { "" } else { &size_str },
            modified_str
        );

        // Style based on hover state and file type
        let style = if Some(idx) == ui_state.hover_file {
            if file.is_dir {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED)
            }
        } else if file.is_dir {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        items.push(ListItem::new(Line::from(vec![Span::styled(display_text, style)])));
    }

    let file_list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green))
            .title(" 📂 Files (click to open) ")
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));

    frame.render_widget(file_list, area);
}

/// Renders the input line with tokenized command
fn render_input(frame: &mut Frame, area: Rect, input_state: &InputState, ui_state: &UiState) {
    let mut spans = Vec::new();

    // Render each token with appropriate styling
    for (idx, token) in input_state.tokens.iter().enumerate() {
        let style = if Some(idx) == ui_state.editing_token {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED)
        } else if Some(idx) == ui_state.hover_token {
            Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default()
        };

        spans.push(Span::styled(token.text.clone(), style));

        // Add space between tokens (except after the last one)
        if idx < input_state.tokens.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }

    // If we're editing a token, show the cursor
    if ui_state.editing_token.is_some() {
        spans.push(Span::styled("|", Style::default().fg(Color::Yellow)));
    }

    let input_widget = Paragraph::new(Line::from(spans))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" 💻 Command ")
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));

    frame.render_widget(input_widget, area);
}

/// Renders the history sidebar
fn render_history(frame: &mut Frame, area: Rect, history: &History) {
    let history_items: Vec<ListItem> = history.commands
        .iter()
        .map(|cmd| ListItem::new(cmd.clone()))
        .collect();

    let history_widget = List::new(history_items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Magenta))
            .title(" 📜 History ")
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .highlight_style(Style::default().fg(Color::Yellow));

    frame.render_widget(history_widget, area);
}

/// Determines which token was clicked based on mouse coordinates
pub fn get_token_at_position(
    input_state: &InputState,
    x: u16,
    input_area: Rect,
) -> Option<usize> {
    // Account for the border and any padding
    let effective_x = x.saturating_sub(input_area.x + 1);

    let mut current_pos = 0;

    for (idx, token) in input_state.tokens.iter().enumerate() {
        let token_width = token.text.width() as u16;

        if effective_x >= current_pos && effective_x < current_pos + token_width {
            return Some(idx);
        }

        // Move past this token and the space after it
        current_pos += token_width + 1;
    }

    None
}

/// Determines which file was clicked based on mouse coordinates
pub fn get_file_at_position(
    ui_state: &UiState,
    y: u16,
    file_area: Rect,
) -> Option<usize> {
    // Account for the border and any padding
    let effective_y = y.saturating_sub(file_area.y + 1);

    // Each file takes up one line
    let idx = effective_y as usize;

    if idx < ui_state.files.len() {
        Some(idx)
    } else {
        None
    }
}

/// Update the file list based on the current directory
pub fn update_file_list(ui_state: &mut UiState) -> anyhow::Result<()> {
    let current_dir = &ui_state.current_dir;
    let mut files = Vec::new();

    // Read the directory entries
    for entry in fs::read_dir(current_dir)? {
        if let Ok(entry) = entry {
            let path = entry.path();
            let file_info = FileInfo::from_path(&path);
            files.push(file_info);
        }
    }

    // Sort directories first, then by name
    files.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });

    ui_state.files = files;
    Ok(())
}

/// Renders the status bar
fn render_status_bar(frame: &mut Frame, area: Rect, ui_state: &UiState) {
    // Get current time
    let now = chrono::Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    // Get current directory in breadcrumb style
    let path = ui_state.current_dir.display().to_string();
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let breadcrumb = if parts.is_empty() {
        "/".to_string()
    } else {
        parts.join(" > ")
    };

    // Create spinner for running commands
    let spinner = if ui_state.is_running {
        match ui_state.spinner_frame {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            3 => "⠸",
            4 => "⠼",
            5 => "⠴",
            6 => "⠦",
            7 => "⠧",
            _ => "⠇",
        }
    } else {
        "✓"
    };

    // Create status line
    let left_part = format!(" {} {}", spinner, breadcrumb);
    let right_part = format!("{} ", time_str);

    // Calculate padding
    let padding_len = area.width as usize - left_part.len() - right_part.len();
    let padding = " ".repeat(padding_len);

    // Create spans
    let spans = vec![
        Span::styled(left_part, Style::default().fg(Color::White).bg(Color::Blue)),
        Span::styled(padding, Style::default().bg(Color::DarkGray)),
        Span::styled(right_part, Style::default().fg(Color::White).bg(Color::Blue)),
    ];

    // Create paragraph
    let status_bar = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Left);

    frame.render_widget(status_bar, area);
}

/// Renders the sudo password prompt
fn render_sudo_password_prompt(frame: &mut Frame, size: Rect, ui_state: &UiState) {
    // Create a centered box for the password prompt
    let width = 50;
    let height = 5;
    let x = (size.width.saturating_sub(width)) / 2;
    let y = (size.height.saturating_sub(height)) / 2;

    let area = Rect::new(x, y, width, height);

    // Create the password field with masked input
    let masked_password = "*".repeat(ui_state.sudo_password.len());
    let password_text = format!("Password: {}", masked_password);

    let password_widget = Paragraph::new(password_text)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Red))
            .title(" 🔒 Sudo Password Required ")
            .title_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left);

    // Render the password prompt
    frame.render_widget(password_widget, area);
}
