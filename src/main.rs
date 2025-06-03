use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::{
    io,
    time::{Duration, Instant},
};

mod executor;
mod history;
mod input;
mod ui;

use executor::Executor;
use history::History;
use input::InputState;
use ui::UiState;

/// Application state
struct App {
    /// UI state
    ui_state: UiState,
    /// Input state
    input_state: InputState,
    /// History manager
    history: History,
    /// Command executor
    executor: Executor,
    /// Whether the application should exit
    should_quit: bool,
}

impl App {
    /// Create a new application
    fn new() -> Result<Self> {
        // Load history
        let history = History::load_default()?;

        Ok(Self {
            ui_state: UiState::default(),
            input_state: InputState::new(),
            history,
            executor: Executor::new(),
            should_quit: false,
        })
    }

    /// Run the application
    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        let tick_rate = Duration::from_millis(100);
        let mut last_tick = Instant::now();

        // Initialize the file list
        ui::update_file_list(&mut self.ui_state)?;

        // Main event loop
        loop {
            // Draw the UI
            terminal.draw(|f| ui::render(f, &mut self.ui_state, &self.input_state, &self.history))?;

            // Check if we should exit
            if self.should_quit {
                return Ok(());
            }

            // Handle events
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if crossterm::event::poll(timeout)? {
                self.handle_event(event::read()?)?;
            }

            // Check for command output
            if self.executor.check_output() {
                // Update the UI with new output
                self.ui_state.output = self.executor.all_output();
                self.ui_state.is_running = self.executor.is_running();

                // If the command was a cd, update the file list
                if !self.ui_state.is_running && self.ui_state.output.iter().any(|line| line.starts_with("Changed directory to:")) {
                    // Update the current directory
                    self.ui_state.current_dir = std::env::current_dir()?;

                    // Update the file list
                    ui::update_file_list(&mut self.ui_state)?;
                }
            }

            // Check if it's time for a tick
            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }

            // Check if the UI needs to be refreshed
            if self.ui_state.needs_refresh {
                // Reset the flag
                self.ui_state.needs_refresh = false;

                // Force a UI refresh
                terminal.draw(|f| ui::render(f, &mut self.ui_state, &self.input_state, &self.history))?;
            }
        }
    }

    /// Handle an event
    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => self.handle_key_event(key)?,
            Event::Mouse(mouse) => self.handle_mouse_event(mouse)?,
            _ => {}
        }

        Ok(())
    }

    /// Handle a key event
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        // Check if we're waiting for a sudo password
        if self.ui_state.sudo_password_prompt {
            match key.code {
                KeyCode::Esc => {
                    // Cancel sudo password prompt
                    self.ui_state.sudo_password_prompt = false;
                    self.ui_state.sudo_password.clear();
                    self.ui_state.sudo_command = None;

                    // Set the needs_refresh flag to trigger a UI update
                    self.ui_state.needs_refresh = true;
                }
                KeyCode::Enter => {
                    // Submit the password
                    if let Some(cmd) = self.ui_state.sudo_command.take() {
                        // Execute the command with the password
                        let password = self.ui_state.sudo_password.clone();
                        self.ui_state.sudo_password.clear();
                        self.ui_state.sudo_password_prompt = false;

                        // Execute the command with the password
                        self.executor.execute_sudo(&cmd, &password)?;
                        self.ui_state.is_running = true;

                        // Set the needs_refresh flag to trigger a UI update
                        self.ui_state.needs_refresh = true;
                    }
                }
                KeyCode::Char(c) => {
                    // Add character to the password
                    self.ui_state.sudo_password.push(c);
                    // Set the needs_refresh flag to trigger a UI update
                    self.ui_state.needs_refresh = true;
                }
                KeyCode::Backspace => {
                    // Remove character from the password
                    self.ui_state.sudo_password.pop();
                    // Set the needs_refresh flag to trigger a UI update
                    self.ui_state.needs_refresh = true;
                }
                _ => {}
            }

            return Ok(());
        }

        // Check if we're editing a token
        if let Some(idx) = self.ui_state.editing_token {
            match key.code {
                KeyCode::Esc => {
                    // Cancel editing
                    self.input_state.cancel_edit();
                    self.ui_state.editing_token = None;
                }
                KeyCode::Enter => {
                    // Commit the edit
                    self.input_state.commit_edit(idx)?;
                    self.ui_state.editing_token = None;
                }
                KeyCode::Char(c) => {
                    // Add character to the token
                    let mut new_text = self.input_state.editing.clone().unwrap_or_default();
                    new_text.push(c);
                    self.input_state.update_editing(new_text);
                }
                KeyCode::Backspace => {
                    // Remove character from the token
                    if let Some(mut text) = self.input_state.editing.clone() {
                        text.pop();
                        self.input_state.update_editing(text);
                    }
                }
                _ => {}
            }

            return Ok(());
        }

        // Global key handlers
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+C: Exit the application
                self.should_quit = true;
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+L: Clear the screen
                self.ui_state.output.clear();
            }
            KeyCode::F(2) => {
                // F2: Toggle history sidebar
                self.ui_state.show_history = !self.ui_state.show_history;
            }
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+H: Alternative way to toggle history sidebar
                self.ui_state.show_history = !self.ui_state.show_history;
            }
            KeyCode::Enter => {
                // Enter: Execute the command
                let command = self.input_state.get_command();
                if !command.trim().is_empty() {
                    // Add to history
                    self.history.add(command.clone());

                    // Check if this is a sudo command
                    if command.trim().starts_with("sudo ") {
                        // Prompt for password
                        self.ui_state.sudo_password_prompt = true;
                        self.ui_state.sudo_command = Some(command.clone());
                    } else {
                        // Execute the command
                        self.executor.execute(&command)?;
                        self.ui_state.is_running = true;
                    }

                    // Clear the input
                    self.input_state.clear();
                }
            }
            KeyCode::Up => {
                // Up: Navigate history backward
                if let Some(prev_cmd) = self.history.previous() {
                    self.input_state.set_input(prev_cmd.clone())?;
                }
            }
            KeyCode::Down => {
                // Down: Navigate history forward
                if let Some(next_cmd) = self.history.next() {
                    self.input_state.set_input(next_cmd.clone())?;
                } else {
                    // End of history, clear the input
                    self.input_state.clear();
                }
            }
            KeyCode::Char(c) => {
                // Add character to the input
                let mut new_input = self.input_state.raw_input.clone();
                new_input.push(c);
                self.input_state.set_input(new_input)?;
            }
            KeyCode::Backspace => {
                // Remove character from the input
                let mut new_input = self.input_state.raw_input.clone();
                new_input.pop();
                self.input_state.set_input(new_input)?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle a mouse event
    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::Down(_) => {
                // Get the terminal size
                let size = crossterm::terminal::size()?;
                let term_rect = ratatui::layout::Rect::new(0, 0, size.0, size.1);

                // Calculate layout using the same function as rendering
                let (main_area, _, input_area, history_area) = ui::calculate_layout(term_rect, self.ui_state.show_history);

                // Calculate output and file list areas
                let (_output_area, file_list_area) = if main_area.height >= 6 { // Minimum height for both sections
                    let output_height = std::cmp::max(3, (main_area.height as f32 * 0.6) as u16);
                    let file_list_height = std::cmp::max(3, main_area.height.saturating_sub(output_height));

                    let chunks = ratatui::layout::Layout::default()
                        .direction(ratatui::layout::Direction::Vertical)
                        .constraints([
                            ratatui::layout::Constraint::Min(output_height),
                            ratatui::layout::Constraint::Min(file_list_height),
                        ])
                        .split(main_area);

                    (Some(chunks[0]), Some(chunks[1]))
                } else {
                    (Some(main_area), None)
                };

                if mouse.row >= input_area.y && mouse.row < input_area.y + input_area.height {
                    // Click in the input area
                    if let Some(token_idx) = ui::get_token_at_position(
                        &self.input_state,
                        mouse.column,
                        input_area,
                    ) {
                        // Start editing the token
                        self.input_state.start_editing(token_idx)?;
                        self.ui_state.editing_token = Some(token_idx);
                    }
                } else if let Some(file_area) = file_list_area {
                    if mouse.row >= file_area.y && mouse.row < file_area.y + file_area.height {
                        // Click in the file list area
                        let effective_file_area = if self.ui_state.show_history {
                            // Adjust width if history sidebar is shown
                            ratatui::layout::Rect::new(
                                file_area.x,
                                file_area.y,
                                file_area.width,
                                file_area.height
                            )
                        } else {
                            file_area
                        };

                        if let Some(file_idx) = ui::get_file_at_position(&self.ui_state, mouse.row, effective_file_area) {
                            let file = &self.ui_state.files[file_idx];

                            if file.is_dir {
                                // Click on a directory - cd into it
                                let cd_command = format!("cd {}", file.name);
                                self.history.add(cd_command.clone());
                                self.executor.execute(&cd_command)?;
                                self.ui_state.is_running = true;
                                self.input_state.clear();

                                // Set the needs_refresh flag to trigger a UI update
                                self.ui_state.needs_refresh = true;
                            } else {
                                // Click on a file - open with editor
                                let edit_command = format!("sudo nano {}", file.name);
                                self.history.add(edit_command.clone());

                                // Prompt for password since this is a sudo command
                                self.ui_state.sudo_password_prompt = true;
                                self.ui_state.sudo_command = Some(edit_command.clone());
                                self.input_state.clear();

                                // Set the needs_refresh flag to trigger a UI update
                                self.ui_state.needs_refresh = true;
                            }
                        }
                    }
                } else if let Some(history_area) = history_area {
                    if mouse.row >= history_area.y && mouse.row < history_area.y + history_area.height {
                        // Click in the history sidebar
                        let history_idx = (mouse.row - history_area.y) as usize;
                        if history_idx < self.history.len() {
                            if let Some(cmd) = self.history.get(history_idx) {
                                self.input_state.set_input(cmd.clone())?;
                            }
                        }
                    }
                }
            }
            MouseEventKind::Moved => {
                // Get the terminal size
                let size = crossterm::terminal::size()?;
                let term_rect = ratatui::layout::Rect::new(0, 0, size.0, size.1);

                // Calculate layout using the same function as rendering
                let (main_area, _, input_area, _) = ui::calculate_layout(term_rect, self.ui_state.show_history);

                // Calculate output and file list areas
                let (_, file_list_area) = if main_area.height >= 6 { // Minimum height for both sections
                    let output_height = std::cmp::max(3, (main_area.height as f32 * 0.6) as u16);
                    let file_list_height = std::cmp::max(3, main_area.height.saturating_sub(output_height));

                    let chunks = ratatui::layout::Layout::default()
                        .direction(ratatui::layout::Direction::Vertical)
                        .constraints([
                            ratatui::layout::Constraint::Min(output_height),
                            ratatui::layout::Constraint::Min(file_list_height),
                        ])
                        .split(main_area);

                    (Some(chunks[0]), Some(chunks[1]))
                } else {
                    (Some(main_area), None)
                };

                if mouse.row >= input_area.y && mouse.row < input_area.y + input_area.height {
                    // Mouse over the input area
                    self.ui_state.hover_token = ui::get_token_at_position(
                        &self.input_state,
                        mouse.column,
                        input_area,
                    );
                    self.ui_state.hover_file = None;
                } else if let Some(file_area) = file_list_area {
                    if mouse.row >= file_area.y && mouse.row < file_area.y + file_area.height {
                        // Mouse over the file list area
                        let effective_file_area = if self.ui_state.show_history {
                            // Adjust width if history sidebar is shown
                            ratatui::layout::Rect::new(
                                file_area.x,
                                file_area.y,
                                file_area.width,
                                file_area.height
                            )
                        } else {
                            file_area
                        };

                        self.ui_state.hover_file = ui::get_file_at_position(&self.ui_state, mouse.row, effective_file_area);
                        self.ui_state.hover_token = None;
                    } else {
                        self.ui_state.hover_token = None;
                        self.ui_state.hover_file = None;
                    }
                } else {
                    self.ui_state.hover_token = None;
                    self.ui_state.hover_file = None;
                }
            }
            _ => {}
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // Run app
    let result = app.run(&mut terminal);

    // Save history before exiting
    let _ = app.history.save();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Print any error
    if let Err(err) = result {
        println!("{:?}", err);
    }

    Ok(())
}
