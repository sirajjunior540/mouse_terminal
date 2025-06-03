use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// Result of command execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Exit code of the command
    pub exit_code: Option<i32>,
    /// Standard output lines
    pub stdout: Vec<String>,
    /// Standard error lines
    pub stderr: Vec<String>,
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

/// Command executor
pub struct Executor {
    /// Channel for receiving command output
    output_rx: Option<Receiver<ExecutionOutput>>,
    /// Channel for sending termination signal
    terminate_tx: Option<Sender<()>>,
    /// Current execution result
    result: ExecutionResult,
    /// Timestamp of the last sudo command (for caching)
    sudo_timestamp: Option<std::time::Instant>,
}

/// Type of output from command execution
#[derive(Debug, Clone)]
pub enum ExecutionOutput {
    /// Standard output line
    Stdout(String),
    /// Standard error line
    Stderr(String),
    /// Command finished with exit code
    Finished(Option<i32>),
}

impl Default for Executor {
    fn default() -> Self {
        Self {
            output_rx: None,
            terminate_tx: None,
            result: ExecutionResult::default(),
            sudo_timestamp: None,
        }
    }
}

impl Executor {
    /// Create a new command executor
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle the cd command
    fn handle_cd_command(&mut self, args: &[String]) -> Result<()> {
        // Get the target directory
        let target_dir = if args.is_empty() {
            // If no arguments, cd to home directory
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
        } else {
            std::path::PathBuf::from(&args[0])
        };

        // Change the current directory
        match std::env::set_current_dir(&target_dir) {
            Ok(_) => {
                // Success - add current directory to output
                let current_dir = std::env::current_dir()?;
                self.result.stdout.push(format!("Changed directory to: {}", current_dir.display()));

                // Automatically run ls after changing directory
                if let Ok(ls_output) = self.run_command_sync("ls", &[]) {
                    // Add ls output to the result
                    self.result.stdout.extend(ls_output.stdout);
                    self.result.stderr.extend(ls_output.stderr);
                }

                self.result.exit_code = Some(0);
            },
            Err(e) => {
                // Error - add error message to output
                self.result.stderr.push(format!("Failed to change directory: {}", e));
                self.result.exit_code = Some(1);
            }
        }

        Ok(())
    }

    /// Execute a command asynchronously
    pub fn execute(&mut self, command: &str) -> Result<()> {
        // Cancel any running command
        self.terminate();

        // Reset the result
        self.result = ExecutionResult::default();

        // Clone the command string to avoid borrowing issues
        let command = command.to_string();

        // Split the command into program and arguments
        let mut parts = command.split_whitespace();
        let program = parts.next().ok_or_else(|| anyhow::anyhow!("Empty command"))?.to_string();
        let args: Vec<String> = parts.map(|s| s.to_string()).collect();

        // Handle built-in commands
        if program == "cd" {
            return self.handle_cd_command(&args);
        }

        // Handle sudo command
        if program == "sudo" && !args.is_empty() {
            // Check if we have a valid sudo session
            if self.is_sudo_session_valid() {
                // Update the sudo timestamp
                self.sudo_timestamp = Some(std::time::Instant::now());
            }
        }

        // Create channels for communication
        let (output_tx, output_rx) = mpsc::channel();
        let (terminate_tx, terminate_rx) = mpsc::channel();

        self.output_rx = Some(output_rx);
        self.terminate_tx = Some(terminate_tx);

        // Spawn a thread to run the command
        thread::spawn(move || {
            let result = Self::run_command(&program, &args, output_tx.clone(), terminate_rx);

            if let Err(e) = result {
                // Send the error as stderr
                let _ = output_tx.send(ExecutionOutput::Stderr(format!("Error: {}", e)));
                let _ = output_tx.send(ExecutionOutput::Finished(Some(-1)));
            }
        });

        Ok(())
    }

    /// Execute a sudo command with a password
    pub fn execute_sudo(&mut self, command: &str, password: &str) -> Result<()> {
        // Cancel any running command
        self.terminate();

        // Reset the result
        self.result = ExecutionResult::default();

        // Clone the command string to avoid borrowing issues
        let command = command.to_string();
        let password = password.to_string();

        // Create channels for communication
        let (output_tx, output_rx) = mpsc::channel();
        let (terminate_tx, terminate_rx) = mpsc::channel();

        self.output_rx = Some(output_rx);
        self.terminate_tx = Some(terminate_tx);

        // Update the sudo timestamp
        self.sudo_timestamp = Some(std::time::Instant::now());

        // Spawn a thread to run the command
        thread::spawn(move || {
            // Create a command that uses sudo with password from stdin
            let mut cmd = Command::new("sudo");
            cmd.arg("-S") // Read password from stdin
                .args(command.split_whitespace().skip(1)) // Skip the "sudo" part
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            // Start the command
            match cmd.spawn() {
                Ok(mut child) => {
                    // Write the password to stdin
                    if let Some(mut stdin) = child.stdin.take() {
                        if let Err(e) = stdin.write_all(format!("{}\n", password).as_bytes()) {
                            let _ = output_tx.send(ExecutionOutput::Stderr(format!("Failed to write password: {}", e)));
                            let _ = output_tx.send(ExecutionOutput::Finished(Some(-1)));
                            return;
                        }
                    }

                    // Get stdout and stderr
                    let stdout = child.stdout.take().unwrap_or_else(|| panic!("Failed to capture stdout"));
                    let stderr = child.stderr.take().unwrap_or_else(|| panic!("Failed to capture stderr"));

                    // Create readers
                    let stdout_reader = BufReader::new(stdout);
                    let stderr_reader = BufReader::new(stderr);

                    // Clone the sender for the threads
                    let stderr_tx = output_tx.clone();
                    let stdout_tx = output_tx.clone();

                    // Spawn a thread to read stdout
                    let stdout_thread = thread::spawn(move || {
                        for line in stdout_reader.lines() {
                            if let Ok(line) = line {
                                if stdout_tx.send(ExecutionOutput::Stdout(line)).is_err() {
                                    break;
                                }
                            }
                        }
                    });

                    // Spawn a thread to read stderr
                    let stderr_thread = thread::spawn(move || {
                        for line in stderr_reader.lines() {
                            if let Ok(line) = line {
                                if stderr_tx.send(ExecutionOutput::Stderr(line)).is_err() {
                                    break;
                                }
                            }
                        }
                    });

                    // Wait for the command to finish or be terminated
                    let exit_status = loop {
                        // Check if we should terminate
                        if terminate_rx.try_recv().is_ok() {
                            // Kill the process
                            let _ = child.kill();
                            break None;
                        }

                        // Check if the process has finished
                        match child.try_wait() {
                            Ok(Some(status)) => break Some(status),
                            Ok(None) => {
                                // Process still running, sleep a bit
                                thread::sleep(std::time::Duration::from_millis(10));
                            }
                            Err(_) => break None,
                        }
                    };

                    // Wait for the reader threads to finish
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();

                    // Send the finished message
                    let exit_code = exit_status.and_then(|s| s.code());
                    let _ = output_tx.send(ExecutionOutput::Finished(exit_code));
                }
                Err(e) => {
                    // Send the error as stderr
                    let _ = output_tx.send(ExecutionOutput::Stderr(format!("Failed to start command: {}", e)));
                    let _ = output_tx.send(ExecutionOutput::Finished(Some(-1)));
                }
            }
        });

        Ok(())
    }

    /// Check if the sudo session is still valid
    pub fn is_sudo_session_valid(&self) -> bool {
        if let Some(timestamp) = self.sudo_timestamp {
            // Sudo session is valid for 15 minutes
            timestamp.elapsed() < std::time::Duration::from_secs(15 * 60)
        } else {
            false
        }
    }

    /// Run a command and capture its output
    fn run_command(
        program: &str,
        args: &[String],
        output_tx: Sender<ExecutionOutput>,
        terminate_rx: Receiver<()>,
    ) -> Result<()> {
        // Create the command
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Start the command
        let mut child = cmd.spawn()?;

        // Get stdout and stderr
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        // Create readers
        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        // Clone the sender for the threads
        let stderr_tx = output_tx.clone();
        let stdout_tx = output_tx.clone();

        // Spawn a thread to read stdout
        let stdout_thread = thread::spawn(move || {
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    if stdout_tx.send(ExecutionOutput::Stdout(line)).is_err() {
                        break;
                    }
                }
            }
        });

        // Spawn a thread to read stderr
        let stderr_thread = thread::spawn(move || {
            for line in stderr_reader.lines() {
                if let Ok(line) = line {
                    if stderr_tx.send(ExecutionOutput::Stderr(line)).is_err() {
                        break;
                    }
                }
            }
        });

        // Wait for the command to finish or be terminated
        let exit_status = loop {
            // Check if we should terminate
            if terminate_rx.try_recv().is_ok() {
                // Kill the process
                let _ = child.kill();
                break None;
            }

            // Check if the process has finished
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    // Process still running, sleep a bit
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break None,
            }
        };

        // Wait for the reader threads to finish
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();

        // Send the finished message
        let exit_code = exit_status.and_then(|s| s.code());
        let _ = output_tx.send(ExecutionOutput::Finished(exit_code));

        Ok(())
    }

    /// Check for new output from the command
    pub fn check_output(&mut self) -> bool {
        let mut updated = false;
        let mut finished = false;
        let mut exit_code = None;

        // Process all available output
        if let Some(rx) = &self.output_rx {
            // Check for new output
            while let Ok(output) = rx.try_recv() {
                match output {
                    ExecutionOutput::Stdout(line) => {
                        self.result.stdout.push(line);
                        updated = true;
                    }
                    ExecutionOutput::Stderr(line) => {
                        self.result.stderr.push(line);
                        updated = true;
                    }
                    ExecutionOutput::Finished(code) => {
                        exit_code = code;
                        finished = true;
                        updated = true;
                    }
                }
            }
        }

        // Handle finished state after processing all output
        if finished {
            self.result.exit_code = exit_code;
            self.output_rx = None;
            self.terminate_tx = None;
        }

        updated
    }

    /// Terminate the running command
    pub fn terminate(&mut self) {
        if let Some(tx) = self.terminate_tx.take() {
            let _ = tx.send(());
        }

        self.output_rx = None;
    }

    /// Check if a command is currently running
    pub fn is_running(&self) -> bool {
        self.output_rx.is_some()
    }

    /// Get the current execution result
    #[allow(dead_code)]
    pub fn result(&self) -> &ExecutionResult {
        &self.result
    }

    /// Get all output lines (stdout and stderr combined)
    pub fn all_output(&self) -> Vec<String> {
        let mut output = Vec::new();

        // Add stdout
        output.extend(self.result.stdout.clone());

        // Add stderr
        output.extend(self.result.stderr.clone());

        output
    }

    /// Run a command synchronously and return its output
    fn run_command_sync(&self, program: &str, args: &[String]) -> Result<ExecutionResult> {
        let mut result = ExecutionResult::default();

        // Create the command
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Start the command
        let output = cmd.output()?;

        // Process stdout
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            result.stdout = stdout.lines().map(|s| s.to_string()).collect();
        }

        // Process stderr
        if let Ok(stderr) = String::from_utf8(output.stderr) {
            result.stderr = stderr.lines().map(|s| s.to_string()).collect();
        }

        // Set exit code
        result.exit_code = output.status.code();

        Ok(result)
    }
}
