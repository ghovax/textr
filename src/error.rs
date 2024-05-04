// #![deny(clippy::unwrap_used, clippy::expect_used)]

use serde::{Deserialize, Serialize};

/// A struct that represents an error with a context and possibly the propagated source error.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContextError {
    /// The context of the error.
    pub context: String,
    /// The propagated source error.
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

// Implement the `std::error::Error` trait for `ContextError` in order for it to be
// used in contexts where the trait is implemented, which is ubiquitous in most libraries
impl std::error::Error for ContextError {}

impl ContextError {
    /// Create a new `ContextError` with the given context, but no source error.
    pub fn with_context<S: Into<String>>(context: S) -> ContextError {
        ContextError {
            context: context.into(),
            source_error: None,
        }
    }

    /// Create a new `ContextError` with the given context and source error.
    pub fn with_error<S: Into<String>>(context: S, error: &dyn std::error::Error) -> ContextError {
        ContextError {
            context: context.into(),
            source_error: Some(error.to_string()),
        }
    }
}

/// Minimizes the first letter of a string. It is used for standardizing the error message in the `ContextError` struct.
fn minimize_first_letter(string: String) -> String {
    // Obtain an iterator over the characters of the string
    let mut characters = string.chars();
    // Get the first character of the string...
    match characters.next() {
        None => String::new(),
        // ...and convert it to lowercase, then chain the rest of the characters to it and collect them as a string
        Some(character) => character.to_lowercase().chain(characters).collect(),
    }
}
