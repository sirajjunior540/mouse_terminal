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

        // Main event loop
        loop {
            // Draw the UI
            terminal.draw(|f| ui::render(f, &self.ui_state, &self.input_state, &self.history))?;

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
            }

            // Check if it's time for a tick
            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
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
            KeyCode::Enter => {
                // Enter: Execute the command
                let command = self.input_state.get_command();
                if !command.trim().is_empty() {
                    // Add to history
                    self.history.add(command.clone());

                    // Execute the command
                    self.executor.execute(&command)?;
                    self.ui_state.is_running = true;

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
                let terminal_height = size.1;

                // Calculate the input area
                let input_start_y = (terminal_height as f32 * 0.85) as u16;

                if mouse.row >= input_start_y {
                    // Click in the input area
                    if let Some(token_idx) = ui::get_token_at_position(
                        &self.input_state,
                        mouse.column,
                        ratatui::layout::Rect::new(0, input_start_y, size.0, terminal_height - input_start_y),
                    ) {
                        // Start editing the token
                        self.input_state.start_editing(token_idx)?;
                        self.ui_state.editing_token = Some(token_idx);
                    }
                } else if self.ui_state.show_history && mouse.column > (size.0 as f32 * 0.7) as u16 {
                    // Click in the history sidebar
                    let history_idx = mouse.row as usize;
                    if history_idx < self.history.len() {
                        if let Some(cmd) = self.history.get(history_idx) {
                            self.input_state.set_input(cmd.clone())?;
                        }
                    }
                }
            }
            MouseEventKind::Moved => {
                // Update hover state
                let size = crossterm::terminal::size()?;
                let terminal_height = size.1;
                let input_start_y = (terminal_height as f32 * 0.85) as u16;

                if mouse.row >= input_start_y {
                    // Mouse over the input area
                    self.ui_state.hover_token = ui::get_token_at_position(
                        &self.input_state,
                        mouse.column,
                        ratatui::layout::Rect::new(0, input_start_y, size.0, terminal_height - input_start_y),
                    );
                } else {
                    self.ui_state.hover_token = None;
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
