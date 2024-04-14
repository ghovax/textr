use rusttype::{point, Point};
use rusttype::{Font, PositionedGlyph, Scale};
use serde::{Deserialize, Serialize};

use std::path::PathBuf;

use unicode_normalization::UnicodeNormalization as _;

use crate::document_configuration::DocumentConfiguration;
use crate::traceable_error::TraceableError;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub root_environment: DocumentContent,
}

impl Document {
    pub fn from_path(document_path: &PathBuf) -> Result<Document, TraceableError> {
        let document_content = std::fs::read_to_string(document_path).map_err(|error| {
            TraceableError::with_source(
                format!("Unable to read the document {:?}", document_path),
                error.into(),
            )
        })?;
        let document: Document = serde_json::from_str(&document_content).map_err(|error| {
            TraceableError::with_source(
                format!("Unable to parse the document {:?}", document_path),
                error.into(),
            )
        })?;

        Ok(document)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged, rename_all = "camelCase")]
pub enum DocumentContent {
    #[serde(rename_all = "camelCase")]
    Environment {
        font_family: String,
        environment_contents: Vec<DocumentContent>,
    },
    #[serde(rename_all = "camelCase")]
    Line {
        initial_caret_position: Vec<f32>,
        line_contents: Vec<DocumentContent>,
    },
    #[serde(rename_all = "camelCase")]
    UnicodeCharacters { text_string: String },
}

impl DocumentContent {
    pub fn layout(
        &self,
        document_configuration: &DocumentConfiguration,
        scale: Scale,
        font: Option<&Font<'static>>,
        caret: &mut Point<f32>,
        positioned_glyphs: &mut Vec<PositionedGlyph<'static>>,
    ) -> Result<(), TraceableError> {
        match self {
            DocumentContent::Environment {
                font_family,
                environment_contents,
            } => {
                let raw_font_data_path = document_configuration.get_font_path(font_family).ok_or(
                    TraceableError::with_context(format!(
                        "Unable to find the font family {:?}",
                        font_family
                    )),
                )?;
                let raw_font_data = std::fs::read(raw_font_data_path.clone()).map_err(|error| {
                    TraceableError::with_source(
                        format!("Unable to read the font data {:?}", raw_font_data_path),
                        error.into(),
                    )
                })?;
                let environment_font = Font::try_from_vec(raw_font_data.to_vec()).ok_or(
                    TraceableError::with_context("Unable to load the font".into()),
                )?;

                for environment_content in environment_contents.iter() {
                    environment_content.layout(
                        document_configuration,
                        scale,
                        Some(&environment_font),
                        caret,
                        positioned_glyphs,
                    )?;
                }
            }
            DocumentContent::Line {
                initial_caret_position,
                line_contents,
            } => {
                *caret = match initial_caret_position[..] {
                    [x, y] => point(x, y),
                    _ => {
                        return Err(TraceableError::with_context(format!(
                            "Invalid initial caret position {:?}",
                            initial_caret_position
                        )))
                    }
                };

                for line_content in line_contents.iter() {
                    line_content.layout(
                        document_configuration,
                        scale,
                        font,
                        caret,
                        positioned_glyphs,
                    )?;
                }
            }
            DocumentContent::UnicodeCharacters { text_string } => {
                let mut last_glyph_id = None;
                let font = font.ok_or(TraceableError::with_context(
                    "Unable to find the font".into(),
                ))?;

                for character in text_string.nfc() {
                    let base_glyph = font.glyph(character);
                    if let Some(id) = last_glyph_id.take() {
                        caret.x += font.pair_kerning(scale, id, base_glyph.id());
                    }
                    last_glyph_id = Some(base_glyph.id());

                    let glyph = base_glyph.scaled(scale).positioned(*caret);
                    caret.x += glyph.unpositioned().h_metrics().advance_width;

                    positioned_glyphs.push(glyph);
                }
            }
        }

        Ok(())
    }
}
