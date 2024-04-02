#![warn(clippy::unwrap_used)]

use itertools::Itertools as _;
use rusttype::Point;
use rusttype::{point, Font, PositionedGlyph, Scale};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization as _;

use crate::{custom_error::CustomError, document_format::TextElement};

pub fn load_fonts(font_styles_map: &mut HashMap<String, FontStyles>) -> Result<(), CustomError> {
    let english_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!(
            "../fonts/Noto_Sans/NotoSans-Regular.ttf"
        ))
        .ok_or(CustomError::with_context("Unable to load the normal english font".into()))?,
        italic_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Italic.ttf")).ok_or(
                CustomError::with_context("Unable to load the italic english font".into()),
            )?,
        ),
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans/NotoSans-Bold.ttf"))
                .ok_or(CustomError::with_context("Unable to load the bold english font".into()))?,
        ),
    };
    font_styles_map.insert("en-US".to_string(), english_font);

    let japanese_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!(
            "../fonts/Noto_Sans_JP/NotoSansJP-Regular.ttf"
        ))
        .ok_or(CustomError::with_context("Unable to load the normal japanese font".into()))?,

        italic_font: None,
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_JP/NotoSansJP-Bold.ttf"))
                .ok_or(CustomError::with_context("Unable to load the bold japanese font".into()))?,
        ),
    };
    font_styles_map.insert("ja-JP".to_string(), japanese_font);

    let simplified_chinese_font = FontStyles {
        normal_font: Font::try_from_bytes(include_bytes!(
            "../fonts/Noto_Sans_SC/NotoSansSC-Regular.ttf"
        ))
        .ok_or(CustomError::with_context(
            "Unable to load the normal simplified chinese font".into(),
        ))?,
        italic_font: None,
        bold_font: Some(
            Font::try_from_bytes(include_bytes!("../fonts/Noto_Sans_SC/NotoSansSC-Bold.ttf"))
                .ok_or(CustomError::with_context(
                    "Unable to load the bold simplified chinese font".into(),
                ))?,
        ),
    };
    font_styles_map.insert("zh-CN".to_string(), simplified_chinese_font);

    Ok(())
}

pub const BORDER_MARGIN: f32 = 20.0;
pub const HEADING_SEPARATION: f32 = 57.0;

#[derive(Clone)]
pub struct FontStyles<'a> {
    pub normal_font: Font<'a>,
    pub italic_font: Option<Font<'a>>,
    pub bold_font: Option<Font<'a>>,
}

pub fn layout_heading<'a>(
    font_styles_map: &HashMap<String, FontStyles<'a>>,
    text_element: &TextElement,
    scale_factor: f32,
    caret: &mut Point<f32>,
) -> Result<Vec<PositionedGlyph<'a>>, CustomError> {
    layout_paragraph(font_styles_map, &vec![text_element.clone()], scale_factor, caret)
}

pub fn layout_paragraph<'a>(
    font_styles_map: &HashMap<String, FontStyles<'a>>,
    text_elements: &Vec<TextElement>,
    scale_factor: f32,
    caret: &mut Point<f32>,
) -> Result<Vec<PositionedGlyph<'a>>, CustomError> {
    let mut positioned_glyphs = Vec::new();

    let max_vertical_ascent = *text_elements
        .iter()
        .filter_map(|text_element| {
            let font_style = match font_styles_map.get(&text_element.language) {
                Some(font_style) => font_style,
                None => {
                    log::error!(
                        "Unable to find the font style for the language {}",
                        text_element.language
                    );
                    return None;
                }
            };
            let font = match text_element.style.font_style.as_str() {
                "bold" => match font_style.bold_font.as_ref() {
                    Some(bold_font) => bold_font,
                    None => {
                        log::error!(
                            "Unable to find the bold font for the language {}",
                            text_element.language
                        );
                        return None;
                    }
                },
                "italic" => match font_style.italic_font.as_ref() {
                    Some(italic_font) => italic_font,
                    None => {
                        log::error!(
                            "Unable to find the italic font for the language {}",
                            text_element.language
                        );
                        return None;
                    }
                },
                "normal" => &font_style.normal_font,
                font_style => {
                    log::error!("Unable to find the font style: {}", font_style);
                    return None;
                }
            };
            let scale = Scale::uniform(text_element.style.font_size as f32 * scale_factor);

            let vertical_metrics = font.v_metrics(scale);
            Some(vertical_metrics.ascent)
        })
        .collect_vec()
        .iter()
        .max_by(|a, b| a.total_cmp(b))
        .ok_or(CustomError::with_context("Unable to find the maximum vertical ascent".into()))?;
    caret.y += max_vertical_ascent;

    for text_element in text_elements {
        let font_style = match font_styles_map.get(&text_element.language) {
            Some(font_style) => font_style,
            None => {
                return Err(CustomError::with_context(format!(
                    "Unable to find the font style for the language {:?}",
                    text_element.language
                )));
            }
        };
        let font = match text_element.style.font_style.as_str() {
            "bold" => match font_style.bold_font.as_ref() {
                Some(bold_font) => bold_font,
                None => {
                    return Err(CustomError::with_context(format!(
                        "Unable to find the bold font for the language {:?}",
                        text_element.language
                    )));
                }
            },
            "italic" => match font_style.italic_font.as_ref() {
                Some(italic_font) => italic_font,
                None => {
                    return Err(CustomError::with_context(format!(
                        "Unable to find the italic font for the language {:?}",
                        text_element.language
                    )));
                }
            },
            "normal" => &font_style.normal_font,
            font_style => {
                return Err(CustomError::with_context(format!(
                    "Unable to find the font style {:?}",
                    font_style
                )));
            }
        };
        let scale = Scale::uniform(text_element.style.font_size as f32 * scale_factor);

        let vertical_metrics = font.v_metrics(scale);
        let advance_height =
            vertical_metrics.ascent - vertical_metrics.descent + vertical_metrics.line_gap;

        let mut last_glyph_id = None;

        for character in text_element.text.chars().nfc() {
            if character.is_control() {
                match character {
                    '\r' | '\n' => {
                        *caret = point(BORDER_MARGIN, caret.y + advance_height);
                    }
                    _ => (),
                }
                continue;
            }
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

    Ok(positioned_glyphs)
}
