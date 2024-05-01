use std::{
    io::Write as _,
    path::{Path, PathBuf},
    str::FromStr as _,
};

use serde::{Deserialize, Serialize};

use crate::{error::TraceableError, pdf::PdfDocument};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub document_id: String,
    pub instance_id: String,
    pub operations: Vec<Operation>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Operation {
    #[serde(rename_all = "camelCase")]
    UnicodeText {
        color: [f32; 3],
        position: [f32; 2],
        text_string: String,
        font_size: f32,
        font_index: usize,
    },
    #[serde(rename_all = "camelCase")]
    AppendNewPage { page_width: f32, page_height: f32 },
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

    pub fn to_pdf(&self) -> Result<PdfDocument, TraceableError> {
        let mut pdf_document = PdfDocument::new(self.document_id.clone());

        let fonts_directory = std::fs::read_dir("fonts/computer-modern")
            .map_err(|error| {
                TraceableError::with_error("Failed to read the fonts directory", &error)
            })?
            .collect::<Vec<_>>();

        let mut font_paths = fonts_directory
            .iter()
            .map(|font_path| {
                font_path.as_ref().map_err(|error| {
                    TraceableError::with_error(
                        format!("Failed to read the font file {:?}", font_path),
                        &error,
                    )
                })
            })
            .collect::<Result<Vec<_>, TraceableError>>()?
            .into_iter()
            .filter(|font_path| font_path.path().extension() == Some("ttf".as_ref()))
            .map(|font_path| font_path.path())
            .collect::<Vec<_>>(); // Need to collect it because of a borrowing requirements

        font_paths.sort();
        let math_font_path = "fonts/lm-math/opentype/latinmodern-math.otf";
        font_paths.push(PathBuf::from_str(math_font_path).map_err(|error| {
            TraceableError::with_error(
                format!("Failed to read the font file {:?}", math_font_path),
                &error,
            )
        })?);

        for font_path in font_paths {
            let _font_index = pdf_document.add_font(&font_path).unwrap();
        }

        let mut current_page_index = 0;
        let mut current_layer_index_in_page = 0;

        for operation in self.operations.iter() {
            match operation {
                Operation::UnicodeText {
                    color,
                    position,
                    text_string,
                    font_size,
                    font_index,
                } => {
                    pdf_document
                        .write_text_to_layer_in_page(
                            current_page_index,
                            current_layer_index_in_page,
                            *color,
                            text_string.clone(),
                            *font_index,
                            *font_size,
                            *position,
                        )
                        .unwrap();
                }
                Operation::AppendNewPage {
                    page_width,
                    page_height,
                } => {
                    let (page_index, layer_index_in_page) =
                        pdf_document.add_page_with_layer(*page_width, *page_height);
                    current_page_index = page_index;
                    current_layer_index_in_page = layer_index_in_page;
                }
            }
        }

        Ok(pdf_document)
    }

    pub fn save_to_pdf_file(&self, path: &Path) -> Result<(), TraceableError> {
        // Note that all documents tend to be heavy so they need to be processed by ps2pdf to be optimized further
        let pdf_document_bytes = self.to_pdf()?.save_to_bytes(self.instance_id.clone())?;
        let mut pdf_file = std::fs::File::create(path).map_err(|error| {
            TraceableError::with_error("Failed to create the output file", &error)
        })?;
        pdf_file
            .write_all(&pdf_document_bytes)
            .map_err(|error| TraceableError::with_error("Failed to save the output file", &error))
            .unwrap();

        Ok(())
    }
}

pub fn optimize_pdf_file_with_ps2pdf(pdf_path: &str) -> Result<(), TraceableError> {
    // Run ps2pdf to optimize the PDF file
    let child = std::process::Command::new("ps2pdf")
        .arg(pdf_path)
        .arg(format!("{}.swp", pdf_path))
        .spawn();
    match child {
        Ok(mut child) => {
            let status = child.wait().map_err(|error| {
                TraceableError::with_error(
                    "Unable to wait for the ps2pdf command execution",
                    &error,
                )
            })?;
            if !status.success() {
                return Err(TraceableError::with_context(format!(
                    "ps2pdf failed with status {:?}",
                    status
                )));
            }
            std::fs::rename(format!("{}.swp", pdf_path), pdf_path).map_err(|error| {
                TraceableError::with_error("Unable to rename the optimized PDF file", &error)
            })?;
        }
        Err(error) => {
            return Err(TraceableError::with_error(
                "Unable to run the ps2pdf command",
                &error,
            ));
        }
    }

    Ok(())
}
pub fn optimize_pdf_file_with_gs(pdf_path: &str) -> Result<(), TraceableError> {
    // Run ghostscript to optimize the PDF file
    // $ gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dPDFSETTINGS=/ebook -dNOPAUSE -dQUIET -dBATCH -sOutputFile=output.pdf input.pdf
    let child = std::process::Command::new("gs")
        .arg("-sDEVICE=pdfwrite")
        .arg("-dCompatibilityLevel=1.5")
        .arg("-dPDFSETTINGS=/ebook")
        .arg("-dNOPAUSE")
        .arg("-dQUIET")
        .arg("-dBATCH")
        .arg(format!("-sOutputFile={}.swp", pdf_path))
        .arg(pdf_path)
        .spawn();
    match child {
        Ok(mut child) => {
            let status = child.wait().map_err(|error| {
                TraceableError::with_error("Unable to wait for the gs command execution", &error)
            })?;
            if !status.success() {
                return Err(TraceableError::with_context(format!(
                    "gs failed with status {:?}",
                    status
                )));
            }
            std::fs::rename(format!("{}.swp", pdf_path), pdf_path).map_err(|error| {
                TraceableError::with_error("Unable to rename the optimized PDF file", &error)
            })?;
        }
        Err(error) => {
            return Err(TraceableError::with_error(
                "Unable to run the gs command",
                &error,
            ));
        }
    }

    Ok(())
}
