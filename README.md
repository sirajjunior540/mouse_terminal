# mouse_term

A cross-platform, mouse-driven terminal emulator built with Rust. This terminal app allows users to click on command-line tokens (words/flags/paths) to edit or replace them instead of moving the caret with arrow keys. It also provides a scrollable, clickable history pane for running or editing previous commands with the mouse.

![mouse_term demo](https://example.com/mouse_term_demo.gif)

## Features

- **Mouse-driven command editing**: Click on any token in the command line to edit it directly
- **Clickable command history**: Access and reuse previous commands with a click
- **File manager view**: Browse files and folders with icons, click to navigate or open files
- **Secure sudo handling**: Password masking and session caching for sudo commands
- **History backup**: Automatic timestamped backups of command history
- **Modern UI**: Powerline-style status bar, loading spinners, and rounded corners
- **Cross-platform support**: Works on macOS, Linux, and Windows
- **Customizable**: Configure colors, keybindings, and more via config file
- **Keyboard parity**: Still supports classic terminal navigation (arrows, Ctrl+A/E, etc.)

## Installation

### Prerequisites

- Rust and Cargo (1.70.0 or newer)
- A terminal that supports ANSI and mouse events

### Building from source

1. Clone the repository:
   ```
   git clone https://github.com/sirajjunior540/mouse_terminal.git
   cd mouse_term
   ```

2. Build the project:
   ```
   cargo build --release
   ```

3. Run the application:
   ```
   cargo run --release
   ```

## Usage

### Basic Navigation

- **Mouse click**: Click on any token to edit it
- **Click on folder**: Navigate to that directory
- **Click on file**: Open the file with sudo nano
- **Enter**: Execute the current command
- **F2** or **Ctrl+H**: Toggle history sidebar
- **Up/Down arrows**: Navigate through command history
- **Ctrl+C**: Exit the application
- **Ctrl+L**: Clear the screen

### Built-in Commands

- **cd [directory]**: Change the current working directory. If no directory is specified, changes to the home directory.

### Configuration

mouse_term can be configured by editing the `config.toml` file in the application directory. This file allows you to customize:

- Color themes (dark/light)
- Keybindings
- Maximum history size

Example configuration:

```toml
[general]
max_history = 500

[colors]
theme = "dark"

[colors.dark]
background = "#1a1a1a"
foreground = "#d0d0d0"
# ... more color settings

[keybindings]
quit = "ctrl+c"
clear_screen = "ctrl+l"
# ... more keybindings
```

## Architecture

mouse_term is built with a modular architecture:

- **ui.rs**: Drawing code and widgets using ratatui
- **input.rs**: Tokenization and inline editor state machine
- **history.rs**: Command history management with load/save functionality and backups
- **executor.rs**: Command execution in child processes, including sudo handling

## New Features

### File Manager View

The terminal now includes a built-in file manager view that:
- Shows files and folders with appropriate icons
- Displays file sizes and modification times
- Allows clicking on folders to navigate to them
- Allows clicking on files to open them with an editor

### Sudo Password Handling

When running commands that require sudo:
- A secure password prompt is displayed
- Password input is masked for security
- Sudo sessions are cached for 15 minutes to avoid repeated password entry

### History Backup

Command history is now automatically backed up:
- Timestamped backups are stored in ~/.mouse_term/history_backups/
- Backups are created whenever history is saved
- History can be restored from backups if needed

### Modern UI

The UI has been modernized with:
- Rounded corners and colorful borders
- A powerline-style status bar showing current directory and time
- Loading spinners for running commands
- Breadcrumb-style directory navigation

## Extending mouse_term

### Multi-line Commands and Pipes

To extend mouse_term to support multi-line commands and pipes in the future:

1. **Multi-line support**:
   - Enhance the `InputState` struct to maintain a vector of lines instead of a single raw input
   - Update the tokenization logic to handle line breaks and continuation characters
   - Modify the UI to display multiple input lines with proper wrapping
   - Add keyboard shortcuts for creating new lines (e.g., Shift+Enter)

2. **Pipe support**:
   - Extend the tokenizer to recognize pipe symbols (`|`) as special tokens
   - Update the executor to handle piped commands by creating multiple processes and connecting their stdin/stdout
   - Add visual indicators in the UI to show pipe connections between commands
   - Implement special handling for clicking on pipe symbols to insert new commands

3. **Implementation approach**:
   - Create a `Command` struct that can represent a single command or a pipeline of commands
   - Implement a parser that converts the tokenized input into a tree of commands
   - Update the executor to handle this command tree structure
   - Enhance the UI to visually represent the command structure

## Development

### Running Tests

```
cargo test
```

### Linting and Formatting

```
cargo fmt
cargo clippy
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [crossterm](https://github.com/crossterm-rs/crossterm) for terminal I/O
- [ratatui](https://github.com/ratatui-org/ratatui) for the TUI framework
