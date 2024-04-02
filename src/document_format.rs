use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::custom_error::CustomError;

#[derive(Debug, Deserialize, Serialize)]
pub struct Document {
    pub root: Vec<Content>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Content {
    Paragraph { contents: Vec<TextElement> },
    Heading { content: TextElement },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct TextElement {
    pub style: Style,
    #[serde(rename = "lang")]
    pub language: String,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Style {
    pub color: String,
    pub font_style: String,
    pub font_size: u32,
}

pub fn load_document(document_path: Option<PathBuf>) -> Result<(Document, PathBuf), CustomError> {
    if document_path.is_none() {
        return Err(CustomError::with_context(
            "No document path provided, you need to provide a path to a document via the `document` flag".into(),
        ));
    }
    #[allow(clippy::unwrap_used)]
    let document_path = document_path.unwrap();
    let document_content = std::fs::read_to_string(&document_path).map_err(|error| {
        CustomError::with_source(
            format!("Unable to read the document {:?}", document_path),
            error.into(),
        )
    })?;
    let document: Document = serde_json::from_str(&document_content).map_err(|error| {
        CustomError::with_source(
            format!("Unable to parse the document {:?}", document_path),
            error.into(),
        )
    })?;

    Ok((document, document_path))
}
