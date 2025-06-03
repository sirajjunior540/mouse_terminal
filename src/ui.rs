use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

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
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_history: false,
            is_running: false,
            output: Vec::new(),
            hover_token: None,
            editing_token: None,
        }
    }
}

/// Renders the entire UI
pub fn render(frame: &mut Frame, ui_state: &UiState, input_state: &InputState, history: &History) {
    let size = frame.size();

    // Create the main layout (vertical split)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(85), // Main viewport
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
        (horizontal_chunks[0], chunks[1])
    } else {
        (chunks[0], chunks[1])
    };

    render_output(frame, main_area, ui_state);
    render_input(frame, input_area, input_state, ui_state);
}

/// Renders the output viewport
fn render_output(frame: &mut Frame, area: Rect, ui_state: &UiState) {
    let output_text: Vec<String> = ui_state.output
        .iter()
        .map(|line| line.clone())
        .collect();

    let output_widget = Paragraph::new(output_text.join("\n"))
        .block(Block::default()
            .borders(Borders::ALL)
            .title("Output"));

    frame.render_widget(output_widget, area);
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
            .title("Command"));

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
            .title("History"))
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
