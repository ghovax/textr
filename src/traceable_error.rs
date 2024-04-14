// #![deny(clippy::unwrap_used, clippy::expect_used)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceableError {
    context: String,
    source: Option<String>,
}

impl std::fmt::Display for TraceableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(source) => write!(
                formatter,
                "{} - {}",
                self.context,
                capitalize_first_letter(source.to_string())
            ),
            None => write!(formatter, "{}", self.context),
        }
    }
}

impl std::error::Error for TraceableError {}

impl TraceableError {
    pub fn with_context(context: String) -> TraceableError {
        TraceableError {
            context,
            source: None,
        }
    }

    pub fn with_source(context: String, source: anyhow::Error) -> TraceableError {
        TraceableError {
            context,
            source: Some(source.to_string()),
        }
    }
}

/// This function capitalizes a string, it is used for standardizing the error message.
pub(crate) fn capitalize_first_letter(string: String) -> String {
    let mut characters = string.chars();
    match characters.next() {
        None => String::new(),
        Some(character) => character.to_uppercase().chain(characters).collect(),
    }
}

/// This function minimizes the first letter of a string, it is used for standardizing the error message.
pub(crate) fn minimize_first_letter(string: String) -> String {
    let mut characters = string.chars();
    match characters.next() {
        None => String::new(),
        Some(character) => character.to_lowercase().chain(characters).collect(),
    }
}
