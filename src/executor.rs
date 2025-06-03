use anyhow::Result;
use std::io::{BufRead, BufReader};
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
        }
    }
}

impl Executor {
    /// Create a new command executor
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Execute a command asynchronously
    pub fn execute(&mut self, command: &str) -> Result<()> {
        // Cancel any running command
        self.terminate();
        
        // Reset the result
        self.result = ExecutionResult::default();
        
        // Split the command into program and arguments
        let mut parts = command.split_whitespace();
        let program = parts.next().ok_or_else(|| anyhow::anyhow!("Empty command"))?;
        let args: Vec<&str> = parts.collect();
        
        // Create channels for communication
        let (output_tx, output_rx) = mpsc::channel();
        let (terminate_tx, terminate_rx) = mpsc::channel();
        
        self.output_rx = Some(output_rx);
        self.terminate_tx = Some(terminate_tx);
        
        // Spawn a thread to run the command
        thread::spawn(move || {
            let result = Self::run_command(program, &args, output_tx.clone(), terminate_rx);
            
            if let Err(e) = result {
                // Send the error as stderr
                let _ = output_tx.send(ExecutionOutput::Stderr(format!("Error: {}", e)));
                let _ = output_tx.send(ExecutionOutput::Finished(Some(-1)));
            }
        });
        
        Ok(())
    }
    
    /// Run a command and capture its output
    fn run_command(
        program: &str,
        args: &[&str],
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
        
        // Clone the sender for the stderr thread
        let stderr_tx = output_tx.clone();
        
        // Spawn a thread to read stdout
        let stdout_thread = thread::spawn(move || {
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    if output_tx.send(ExecutionOutput::Stdout(line)).is_err() {
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
                        self.result.exit_code = code;
                        self.output_rx = None;
                        self.terminate_tx = None;
                        updated = true;
                    }
                }
            }
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
}