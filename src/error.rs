// #![deny(clippy::unwrap_used, clippy::expect_used)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TraceableError {
    pub context: String,
    pub source: Option<String>,
}

impl std::fmt::Display for TraceableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(source) => write!(
                formatter,
                "{}: {}",
                self.context,
                minimize_first_letter(source.to_string())
            ),
            None => write!(formatter, "{}", self.context),
        }
    }
}

impl std::error::Error for TraceableError {}

impl TraceableError {
    pub fn with_context<S: Into<String>>(context: S) -> TraceableError {
        TraceableError {
            context: context.into(),
            source: None,
        }
    }

    pub fn with_error<S: Into<String>>(
        context: S,
        source: &dyn std::error::Error,
    ) -> TraceableError {
        TraceableError {
            context: context.into(),
            source: Some(source.to_string()),
        }
    }
}

/// Minimizes the first letter of a string, it is used for standardizing the error message.
pub(crate) fn minimize_first_letter(string: String) -> String {
    let mut characters = string.chars();
    match characters.next() {
        None => String::new(),
        Some(character) => character.to_lowercase().chain(characters).collect(),
    }
}
