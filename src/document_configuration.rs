// #![allow(non_snake_case)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::traceable_error::TraceableError;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DocumentConfiguration {
    pub page_width: u32,
    pub page_height: u32,
    pub font_size: u32,
    pub global_magnification: f32,
    pub font_associations: Vec<FontAssociation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FontAssociation {
    pub font_family: String,
    pub font_file_path: PathBuf,
}

impl DocumentConfiguration {
    pub fn from_path(
        document_configuration_file_path: &PathBuf,
    ) -> Result<Self, TraceableError> {
        let configuration_file_contents = std::fs::read_to_string(document_configuration_file_path)
            .map_err(|error| {
                TraceableError::with_source(
                    "Failed to read the configuration file".into(),
                    error.into(),
                )
            })?;
        let configuration: DocumentConfiguration =
            serde_json::from_str(&configuration_file_contents).map_err(|error| {
                TraceableError::with_source(
                    "Failed to parse the configuration file".into(),
                    error.into(),
                )
            })?;

        Ok(configuration)
    }

    pub fn get_font_path(&self, font_family: &str) -> Option<PathBuf> {
        self.font_associations
            .iter()
            .find(|font_association| font_association.font_family == font_family)
            .map(|font_association| font_association.font_file_path.clone())
    }
}
