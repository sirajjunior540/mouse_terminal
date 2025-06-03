use anyhow::Result;
use thiserror::Error;
use unicode_width::UnicodeWidthStr;

/// Errors that can occur during input processing
#[derive(Error, Debug)]
pub enum InputError {
    #[error("Invalid token index: {0}")]
    InvalidTokenIndex(usize),
    
    #[error("Unmatched quote in input")]
    UnmatchedQuote,
}

/// Represents a token in the command line
#[derive(Debug, Clone)]
pub struct Token {
    /// The text content of the token
    pub text: String,
    /// The byte range in the original input string
    pub range: (usize, usize),
}

/// State for the input line and editor
pub struct InputState {
    /// The raw input string
    pub raw_input: String,
    /// The tokenized input
    pub tokens: Vec<Token>,
    /// The token currently being edited (if any)
    pub editing: Option<String>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            raw_input: String::new(),
            tokens: Vec::new(),
            editing: None,
        }
    }
}

impl InputState {
    /// Create a new empty input state
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Update the raw input and re-tokenize
    pub fn set_input(&mut self, input: String) -> Result<()> {
        self.raw_input = input;
        self.tokenize()?;
        Ok(())
    }
    
    /// Clear the input
    pub fn clear(&mut self) {
        self.raw_input.clear();
        self.tokens.clear();
        self.editing = None;
    }
    
    /// Start editing a token
    pub fn start_editing(&mut self, token_idx: usize) -> Result<()> {
        if token_idx >= self.tokens.len() {
            return Err(InputError::InvalidTokenIndex(token_idx).into());
        }
        
        self.editing = Some(self.tokens[token_idx].text.clone());
        Ok(())
    }
    
    /// Commit the edited token
    pub fn commit_edit(&mut self, token_idx: usize) -> Result<()> {
        if token_idx >= self.tokens.len() {
            return Err(InputError::InvalidTokenIndex(token_idx).into());
        }
        
        if let Some(edited_text) = self.editing.take() {
            // Update the token text
            self.tokens[token_idx].text = edited_text;
            
            // Rebuild the raw input from tokens
            self.rebuild_raw_input();
        }
        
        Ok(())
    }
    
    /// Cancel the current edit
    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }
    
    /// Update the text of the token being edited
    pub fn update_editing(&mut self, text: String) {
        self.editing = Some(text);
    }
    
    /// Rebuild the raw input string from tokens
    fn rebuild_raw_input(&mut self) {
        self.raw_input = self.tokens
            .iter()
            .map(|token| {
                // Quote the token if it contains whitespace
                if token.text.contains(char::is_whitespace) {
                    format!("\"{}\"", token.text)
                } else {
                    token.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    
    /// Tokenize the raw input into tokens
    fn tokenize(&mut self) -> Result<()> {
        self.tokens.clear();
        
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut in_quotes = false;
        let mut escaped = false;
        let mut start_pos = 0;
        
        for (i, c) in self.raw_input.char_indices() {
            if escaped {
                current_token.push(c);
                escaped = false;
                continue;
            }
            
            match c {
                '\\' => {
                    escaped = true;
                }
                '"' => {
                    in_quotes = !in_quotes;
                }
                ' ' | '\t' if !in_quotes => {
                    if !current_token.is_empty() {
                        tokens.push(Token {
                            text: current_token,
                            range: (start_pos, i),
                        });
                        current_token = String::new();
                        start_pos = i + 1;
                    } else {
                        // Skip consecutive whitespace
                        start_pos = i + 1;
                    }
                }
                _ => {
                    current_token.push(c);
                }
            }
        }
        
        // Check for unmatched quotes
        if in_quotes {
            return Err(InputError::UnmatchedQuote.into());
        }
        
        // Add the last token if there is one
        if !current_token.is_empty() {
            tokens.push(Token {
                text: current_token,
                range: (start_pos, self.raw_input.len()),
            });
        }
        
        self.tokens = tokens;
        Ok(())
    }
    
    /// Get the full command string
    pub fn get_command(&self) -> String {
        self.raw_input.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tokenize_simple() {
        let mut input_state = InputState::new();
        input_state.set_input("ls -la /home".to_string()).unwrap();
        
        assert_eq!(input_state.tokens.len(), 3);
        assert_eq!(input_state.tokens[0].text, "ls");
        assert_eq!(input_state.tokens[1].text, "-la");
        assert_eq!(input_state.tokens[2].text, "/home");
    }
    
    #[test]
    fn test_tokenize_with_quotes() {
        let mut input_state = InputState::new();
        input_state.set_input("echo \"hello world\" test".to_string()).unwrap();
        
        assert_eq!(input_state.tokens.len(), 3);
        assert_eq!(input_state.tokens[0].text, "echo");
        assert_eq!(input_state.tokens[1].text, "\"hello world\"");
        assert_eq!(input_state.tokens[2].text, "test");
    }
}