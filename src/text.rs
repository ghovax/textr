#![allow(dead_code)]

use nalgebra_glm::IVec2;

/// All the configuration and properties of a glyph.
#[derive(Debug, Clone, Default)]
pub struct CharacterGeometry {
    pub size: IVec2,    // Size of glyph
    pub bearing: IVec2, // Offset from baseline to left/top of glyph
    pub advance: u32,   // Offset to advance to the next glyph
}

/// Represents margins around a block of text. The order in which they are read is
/// that the order in which they are present into the struct.
#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl std::str::FromStr for Margins {
    type Err = std::num::ParseFloatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut margins = Margins {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        };
        // Split the string by the comma character
        let mut parts = s.split(',');
        // Read the margins
        margins.top = parts.next().unwrap_or("0").parse::<f32>()?;
        margins.right = parts.next().unwrap_or("0").parse::<f32>()?;
        margins.bottom = parts.next().unwrap_or("0").parse::<f32>()?;
        margins.left = parts.next().unwrap_or("0").parse::<f32>()?;

        Ok(margins)
    }
}

/// Represents a line of text with various properties for formatting and presentation.
struct LineOfText {
    content: Vec<String>,  // The actual content of the text.
    alignment: Alignment,  // The alignment of the text within the line.
    vertical_spacing: f32, // The vertical space between lines of text.
    margins: Margins,      // Margins around the text block.
    indentation: f32,      // The indentation of the first line in the paragraph.
}

/// Enum representing text alignment options.
enum Alignment {
    Left,
    Right,
    Center,
    Justified,
}

/// Enum representing text style options.
enum TextStyle {
    Normal,
    Bold,
    Italic,
    Underline,
    Monospace,
}

impl LineOfText {
    fn new(content: Vec<String>, vertical_spacing: f32, margins: Margins) -> Self {
        LineOfText {
            content,
            alignment: Alignment::Left,
            vertical_spacing,
            margins,
            indentation: 0.0,
        }
    }
}
