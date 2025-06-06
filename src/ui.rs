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
    /// Whether the UI needs to be refreshed
    pub needs_refresh: bool,
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
            needs_refresh: false,
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

/// Calculate the layout for the UI
pub fn calculate_layout(size: Rect, show_history: bool) -> (Rect, Rect, Rect, Option<Rect>) {
    // Ensure minimum height for each section
    let _min_output_height = 5;
    let status_bar_height = 2;
    let min_input_height = 3;

    // Calculate available height
    let available_height = size.height.saturating_sub(status_bar_height);

    // Calculate input height (minimum 3 lines or 15% of available height, whichever is larger)
    let input_height = std::cmp::max(
        min_input_height,
        (available_height as f32 * 0.15) as u16
    );

    // Calculate main viewport height
    let main_height = available_height.saturating_sub(input_height);

    // Create the main layout (vertical split)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(main_height),     // Main viewport
            Constraint::Length(status_bar_height), // Status bar
            Constraint::Min(input_height),    // Input line
        ])
        .split(size);

    // If history sidebar is enabled, create a horizontal split for the main area
    if show_history {
        // Calculate history width (30% of screen width, minimum 20 columns)
        let history_width = std::cmp::max(20, (size.width as f32 * 0.3) as u16);
        let main_width = size.width.saturating_sub(history_width);

        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(main_width),    // Main viewport
                Constraint::Min(history_width), // History sidebar
            ])
            .split(chunks[0]);

        (horizontal_chunks[0], chunks[1], chunks[2], Some(horizontal_chunks[1]))
    } else {
        (chunks[0], chunks[1], chunks[2], None)
    }
}

/// Renders the entire UI
pub fn render(frame: &mut Frame, ui_state: &mut UiState, input_state: &InputState, history: &History) {
    let size = frame.size();

    // Calculate layout
    let (main_area, status_area, input_area, history_area) = calculate_layout(size, ui_state.show_history);

    // Render history if enabled
    if let Some(history_area) = history_area {
        render_history(frame, history_area, history);
    }

    render_output(frame, main_area, ui_state);
    render_status_bar(frame, status_area, ui_state);
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
    // Ensure minimum heights for output and file list
    let min_output_height = 3;
    let min_file_list_height = 3;

    // Calculate available height
    let available_height = area.height;

    // If we have enough space for both sections with minimum heights
    if available_height >= min_output_height + min_file_list_height {
        // Calculate output height (60% of available space, but at least min_output_height)
        let output_height = std::cmp::max(
            min_output_height,
            (available_height as f32 * 0.6) as u16
        );

        // Calculate file list height (remaining space, but at least min_file_list_height)
        let file_list_height = std::cmp::max(
            min_file_list_height,
            available_height.saturating_sub(output_height)
        );

        // Split the area into two parts: output and file list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(output_height),     // Command output
                Constraint::Min(file_list_height),  // File list
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
    } else {
        // Not enough space for both sections, just show output
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

        frame.render_widget(output_widget, area);
    }
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
    // Check if the click is within the input area's horizontal bounds
    if x < input_area.x || x >= input_area.x + input_area.width {
        return None;
    }

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
    // Check if the click is within the file area's vertical bounds
    if y < file_area.y || y >= file_area.y + file_area.height {
        return None;
    }

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
    // Create a semi-transparent overlay for the entire screen
    let overlay = Block::default()
        .style(Style::default().bg(Color::Black).fg(Color::White));
    frame.render_widget(overlay, size);

    // Calculate adaptive dimensions for the password prompt
    // Width: 50% of screen width, but at least 40 columns and at most 80 columns
    let width = std::cmp::min(
        80,
        std::cmp::max(40, (size.width as f32 * 0.5) as u16)
    );

    // Height: 30% of screen height, but at least 5 rows and at most 10 rows
    let height = std::cmp::min(
        10,
        std::cmp::max(5, (size.height as f32 * 0.3) as u16)
    );

    // Ensure the prompt fits on screen
    let width = std::cmp::min(width, size.width.saturating_sub(4));
    let height = std::cmp::min(height, size.height.saturating_sub(4));

    // Center the prompt
    let x = (size.width.saturating_sub(width)) / 2;
    let y = (size.height.saturating_sub(height)) / 2;

    let area = Rect::new(x, y, width, height);

    // Create the password field with masked input
    let masked_password = "*".repeat(ui_state.sudo_password.len());
    let cursor = if ui_state.spinner_frame % 2 == 0 { "█" } else { " " }; // Blinking cursor
    let password_text = format!("Password: {}{}", masked_password, cursor);

    // Add instructions
    let instructions = "\n\nPress Enter to submit or Esc to cancel";

    let password_widget = Paragraph::new(format!("{}{}", password_text, instructions))
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
