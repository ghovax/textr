// #![deny(clippy::unwrap_used, clippy::expect_used)]

use serde::{Deserialize, Serialize};

/// A struct that represents an error with a context and possibly the propagated source error.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContextError {
    pub context: String,
    pub source_error: Option<String>,
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source_error {
            Some(source_error) => write!(
                formatter,
                "{}: {}",
                self.context,
                minimize_first_letter(source_error.to_string()),
            ),
            None => write!(formatter, "{}", self.context),
        }
    }
}

impl std::error::Error for ContextError {}

impl ContextError {
    /// Create a new `ContextError`` with the given context.
    pub fn with_context<S: Into<String>>(context: S) -> ContextError {
        ContextError {
            context: context.into(),
            source_error: None,
        }
    }

    /// Create a new `ContextError`` with the given context and source error.
    pub fn with_error<S: Into<String>>(context: S, error: &dyn std::error::Error) -> ContextError {
        ContextError {
            context: context.into(),
            source_error: Some(error.to_string()),
        }
    }
}

/// Minimizes the first letter of a string, it is used for standardizing the error message.
fn minimize_first_letter(string: String) -> String {
    let mut characters = string.chars();
    match characters.next() {
        None => String::new(),
        Some(character) => character.to_lowercase().chain(characters).collect(),
    }
}
