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

/// Represents margins around a block of text.
#[derive(Debug, Clone, Copy)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
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
