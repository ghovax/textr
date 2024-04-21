use printpdf::{
    Color, Image, ImageTransform, IndirectFontRef, LinkAnnotation, Mm, OffsetDateTime,
    PdfConformance, PdfDocument, PdfDocumentReference, Rgb,
};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, path::PathBuf};

use crate::traceable_error::TraceableError;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub title: String,
    pub author: String,
    pub date_in_unix_timestamp: i64,
    pub page_width: f32,
    pub page_height: f32,
    pub raw_contents: Vec<DocumentContent>,
}

impl Document {
    pub fn from_path(document_path: &PathBuf) -> Result<Self, TraceableError> {
        let document_content = std::fs::read_to_string(document_path).map_err(|error| {
            TraceableError::with_error(
                format!("Unable to read the document {:?}", document_path),
                &error,
            )
        })?;
        let document: Self = serde_json::from_str(&document_content).map_err(|error| {
            TraceableError::with_error(
                format!("Unable to parse the document {:?}", document_path),
                &error,
            )
        })?;

        Ok(document)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum DocumentContent {
    #[serde(rename_all = "camelCase")]
    UnicodeText {
        color: [f32; 3],
        position: [f32; 2],
        text: String,
        font_size: f32,
        font_family: String,
        url: Option<String>,
        highlight_area: Option<[f32; 4]>,
    },
    #[serde(rename_all = "camelCase")]
    Image {
        image_path: String,
        position: [f32; 2],
        scale: [f32; 2],
    },
}

pub fn document_to_pdf(document: &Document) -> Result<PdfDocumentReference, TraceableError> {
    let (pdf_document, page_index, layer_index) = PdfDocument::new(
        document.title.clone(),
        Mm(document.page_width),
        Mm(document.page_height),
        "Layer 1",
    );

    let document_date = OffsetDateTime::from_unix_timestamp(document.date_in_unix_timestamp)
        .map_err(|error| {
            TraceableError::with_error(
                format!(
                    "Unable to parse the date {:?}",
                    document.date_in_unix_timestamp
                ),
                &error,
            )
        })?;

    let pdf_document = pdf_document
        .with_author(document.author.clone())
        .with_creation_date(document_date)
        .with_mod_date(document_date)
        .with_metadata_date(document_date);
    let current_layer = pdf_document.get_page(page_index).get_layer(layer_index);

    let mut fonts_map = std::collections::HashMap::new();
    populate_fonts_map_with_builtin_fonts(&pdf_document, &mut fonts_map)?;
    log::debug!("The available builtin fonts are: {:?}", fonts_map.keys());

    for content in document.raw_contents.iter() {
        match content {
            DocumentContent::UnicodeText {
                color,
                position,
                text,
                font_size,
                font_family,
                url,
                highlight_area,
            } => {
                let font = fonts_map.get(font_family).unwrap();
                let [x, y] = *position;
                let [r, g, b] = *color;

                current_layer.begin_text_section();
                current_layer.set_font(font, *font_size);
                current_layer.set_text_cursor(Mm(x), Mm(y));
                current_layer.write_text(text, font);
                current_layer.set_fill_color(Color::Rgb(Rgb::new(r, g, b, None)));
                current_layer.end_text_section();

                if let Some(url) = url.clone() {
                    let [x, y, maximum_x, maximum_y] =
                        highlight_area.ok_or(TraceableError::with_context(format!(
                            "Unable to find the highlight area for the text {:?}",
                            text
                        )))?;

                    let link_annotation = LinkAnnotation::new(
                        printpdf::Rect::new(Mm(x), Mm(y), Mm(maximum_x), Mm(maximum_y)),
                        Some(printpdf::BorderArray::default()),
                        Some(printpdf::ColorArray::RGB([r, g, b])),
                        printpdf::Actions::uri(url.clone()),
                        Some(printpdf::HighlightingMode::Invert),
                    );
                    current_layer.add_link_annotation(link_annotation);
                }
            }
            DocumentContent::Image {
                image_path,
                position,
                scale,
            } => {
                let image_data = std::fs::read(image_path).map_err(|error| {
                    TraceableError::with_error(
                        format!("Unable to read the image {:?}", image_path),
                        &error,
                    )
                })?;
                let mut reader = Cursor::new(&image_data);
                let decoder = printpdf::image_crate::codecs::png::PngDecoder::new(&mut reader)
                    .map_err(|error| {
                        TraceableError::with_error(
                            format!("Unable to decode the image {:?}", image_path),
                            &error,
                        )
                    })?;
                let image = Image::try_from(decoder).map_err(|error| {
                    TraceableError::with_error(
                        format!("Unable to decode the image {:?}", image_path),
                        &error,
                    )
                })?;

                let [x, y] = *position;
                let [scale_x, scale_y] = *scale;
                image.add_to_layer(
                    current_layer.clone(),
                    ImageTransform {
                        rotate: None,
                        translate_x: Some(Mm(x)),
                        translate_y: Some(Mm(y)),
                        scale_x: Some(scale_x),
                        scale_y: Some(scale_y),
                        dpi: None,
                    },
                );
            }
        }
    }

    pdf_document.check_for_errors().map_err(|error| {
        TraceableError::with_error(
            format!("Unable to render the document {:?}", document.title),
            &error,
        )
    })?;
    let pdf_document = pdf_document
        .with_document_id(document.id.clone())
        .with_conformance(PdfConformance::A2U_2011_PDF_1_7);

    Ok(pdf_document)
}

macro_rules! add_external_font {
    ($fonts_map:ident, $pdf_document:ident, $path:expr, $key:expr) => {
        let font_raw_data = include_bytes!($path);
        let mut font_reader = std::io::Cursor::new(font_raw_data.as_ref());
        let font = $pdf_document.add_external_font_with_subsetting(&mut font_reader, true).map_err(|error| {
            TraceableError::with_error(
                format!("Unable to add the font {:?} to the fonts map in the initialization of the program", $key),
                &error,
            )
        })?;
        $fonts_map.insert($key.to_string(), font);
    };
}

fn populate_fonts_map_with_builtin_fonts(
    pdf_document: &PdfDocumentReference,
    fonts_map: &mut std::collections::HashMap<String, IndirectFontRef>,
) -> Result<(), TraceableError> {
    // Static fonts in the Computer Modern font-family
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbi.ttf",
        "CMU Serif BoldItalic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbl.ttf",
        "CMU Serif Extra BoldSlanted"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbmo.ttf",
        "CMU Bright Oblique"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbmr.ttf",
        "CMU Bright Roman"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbso.ttf",
        "CMU Bright SemiBoldOblique"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbsr.ttf",
        "CMU Bright SemiBold"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbtl.ttf",
        "CMU Typewriter Text Light"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbto.ttf",
        "CMU Typewriter Text LightOblique"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunbx.ttf",
        "CMU Serif Bold"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunci.ttf",
        "CMU Classical Serif Italic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunit.ttf",
        "CMU Typewriter Text Italic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunobi.ttf",
        "CMU Concrete BoldItalic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunobx.ttf",
        "CMU Concrete Bold"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunorm.ttf",
        "CMU Concrete Roman"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunoti.ttf",
        "CMU Concrete Italic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunrm.ttf",
        "CMU Serif Roman"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunsi.ttf",
        "CMU Sans Serif Oblique"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunsl.ttf",
        "CMU Serif Extra RomanSlanted"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunso.ttf",
        "CMU Sans Serif BoldOblique"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunss.ttf",
        "CMU Sans Serif Medium"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunssdc.ttf",
        "CMU Sans Serif DemiCondensed"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunsx.ttf",
        "CMU Sans Serif Bold"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmuntb.ttf",
        "CMU Typewriter Text Bold"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunti.ttf",
        "CMU Serif Italic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmuntt.ttf",
        "CMU Typewriter Text Regular"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmuntx.ttf",
        "CMU Typewriter Text BoldItalic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunui.ttf",
        "CMU Serif Upright Italic UprightItalic"
    );
    // Variable width fonts
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunvi.ttf",
        "CMU Typewriter Text Variable Width Italic"
    );
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/computer-modern/cmunvt.ttf",
        "CMU Typewriter Text Variable Width Medium"
    );
    // Math fonts
    add_external_font!(
        fonts_map,
        pdf_document,
        "../fonts/lm-math/opentype/latinmodern-math.otf",
        "Latin Modern Math Regular"
    );

    Ok(())
}
