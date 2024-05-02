use serde::{Deserialize, Serialize};
use std::{
    io::Write as _,
    path::{Path, PathBuf},
    str::FromStr as _,
};

use crate::{error::ContextError, pdf::PdfDocument};

/// The document metadata and the operations needed in order to construct it
/// are saved into this struct. This can be deserialized from a properly-constructed
/// file in the JSON format.
///
/// # Parameters
///
/// * `document_id` - A string that holds the ID of the document: a unique identifier
/// which when paired with the instance ID (`instance_id`) uniquely identifies a document.
/// Both the parameters are needed for creating a correct PDF document.
/// * `instance_id` - A string that holds the ID of the instance (see `document_id`).
/// * `operations` - A vector of `Operation` structs that holds the operations needed to
/// construct the document. Such operations can be for instance to include some unicode text
/// into the document at a specific position and with the given font, font size and color, or
/// either to append a new page to the document with a given width and height.
///
/// # Example
///
/// See the example `document_to_pdf` in the folder `examples` for how to construct a `Document`
/// from a file in the JSON format which adheres to the `Document` specification.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// The unique ID of the document (to be paired with the instance ID).
    pub document_id: String,
    /// The unique ID of the instance (see the document ID).
    pub instance_id: String,
    /// The operations needed to construct the document.
    pub operations: Vec<Operation>,
}

/// The `Operation` struct is used to represent the operations needed to construct a document.
/// It can be any of the following: `UnicodeText`, `AppendNewPage`.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum Operation {
    /// Represents a piece of text to be rendered in the PDF document.
    #[serde(rename_all = "camelCase")]
    UnicodeText {
        /// The color of the text.
        color: [f32; 3],
        /// The position of the text.
        position: [f32; 2],
        /// The text to be rendered, save the in an UTF-8-compatible format.
        text_string: String,
        /// The font size of the text.
        font_size: f32,
        /// The font index of the text, use the in order to retrieve the proper font.
        /// This is a low-level information and the proper index for the specific use-case
        /// can be calculated by knowing in which order the fonts have been loaded into the document.
        font_index: usize,
    },
    /// Represents a new page with the given width and height to be appended to the PDF document.
    #[serde(rename_all = "camelCase")]
    AppendNewPage {
        /// The width of the new page.
        page_width: f32,
        /// The height of the new page.
        page_height: f32,
    },
}

impl Document {
    /// Creates a new `Document` from the given path by deserializing the JSON document.
    ///
    /// # Arguments
    ///
    /// * `document_path` - The path to the JSON document.
    pub fn from_path(document_path: &PathBuf) -> Result<Self, ContextError> {
        let document_content = std::fs::read_to_string(document_path).map_err(|error| {
            ContextError::with_error(
                format!("Unable to read the document {:?}", document_path),
                &error,
            )
        })?;
        let document: Self = serde_json::from_str(&document_content).map_err(|error| {
            ContextError::with_error(
                format!("Unable to parse the document {:?}", document_path),
                &error,
            )
        })?;

        Ok(document)
    }

    /// Converts the given `Document` into a PDF document (`PdfDocument`). This is done by first loading all the
    /// built-in fonts present in the `fonts` directory of the CMU family, including the math font,
    /// then by iterating over the operations present in the document in order to map them to the associated
    /// operation in a PDF document. This is a high-level function that hides the low-level requirements
    /// and procedures needed for constructing a PDF document by calling the functions defined for `PdfDocument`.
    pub fn to_pdf(&self) -> Result<PdfDocument, ContextError> {
        let mut pdf_document = PdfDocument::new(self.document_id.clone());

        let fonts_directory = std::fs::read_dir("fonts/computer-modern")
            .map_err(|error| {
                ContextError::with_error("Failed to read the fonts directory", &error)
            })?
            .collect::<Vec<_>>();

        let mut font_paths = fonts_directory
            .iter()
            .map(|font_path| {
                font_path.as_ref().map_err(|error| {
                    ContextError::with_error(
                        format!("Failed to read the font file {:?}", font_path),
                        &error,
                    )
                })
            })
            .collect::<Result<Vec<_>, ContextError>>()?
            .into_iter()
            .filter(|font_path| font_path.path().extension() == Some("ttf".as_ref()))
            .map(|font_path| font_path.path())
            .collect::<Vec<_>>(); // Need to collect it because of a borrowing requirements

        font_paths.sort();
        let math_font_path = "fonts/lm-math/opentype/latinmodern-math.otf";
        font_paths.push(PathBuf::from_str(math_font_path).map_err(|error| {
            ContextError::with_error(
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

    /// This is a commodity function that saves the document as a PDF file. This is done by first converting
    /// the document to the `PdfDocument` format and then by saving the PDF document as bytes, which can be
    /// written to any file. Clearly this function requests the file system to create a file at the given path,
    /// which will have the side effects of overwriting any present file at the path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the output PDF file.
    pub fn save_to_pdf_file(&self, path: &Path) -> Result<(), ContextError> {
        // Note that all documents tend to be heavy so they need to be processed by ps2pdf to be optimized further
        let pdf_document_bytes = self.to_pdf()?.save_to_bytes(self.instance_id.clone())?;
        let mut pdf_file = std::fs::File::create(path).map_err(|error| {
            ContextError::with_error("Failed to create the output file", &error)
        })?;
        pdf_file
            .write_all(&pdf_document_bytes)
            .map_err(|error| ContextError::with_error("Failed to save the output file", &error))
            .unwrap();

        Ok(())
    }
}

/// This function is used to optimize the PDF file by running ps2pdf on it.
/// An intermediate file with the `.swp` extension is created and then renamed immediately
/// to the expected one, which is the given path.
///
/// # Arguments
///
/// * `pdf_path` - The path to the PDF file to be optimized.
///
/// This is procedure of creating an intermediate file is a workaround to a limitation of the shell.
pub fn optimize_pdf_file_with_ps2pdf(pdf_path: &str) -> Result<(), ContextError> {
    // Run ps2pdf to optimize the PDF file
    let child = std::process::Command::new("ps2pdf")
        .arg(pdf_path)
        .arg(format!("{}.swp", pdf_path))
        .spawn();
    match child {
        Ok(mut child) => {
            let status = child.wait().map_err(|error| {
                ContextError::with_error("Unable to wait for the ps2pdf command execution", &error)
            })?;
            if !status.success() {
                return Err(ContextError::with_context(format!(
                    "ps2pdf failed with status {:?}",
                    status
                )));
            }
            std::fs::rename(format!("{}.swp", pdf_path), pdf_path).map_err(|error| {
                ContextError::with_error("Unable to rename the optimized PDF file", &error)
            })?;
        }
        Err(error) => {
            return Err(ContextError::with_error(
                "Unable to run the ps2pdf command",
                &error,
            ));
        }
    }

    Ok(())
}

/// This function is used to optimize the PDF file by running ghostscript on it. The command which is run
/// is the following:
///
/// ```bash
/// $ gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dPDFSETTINGS=/ebook -dNOPAUSE -dQUIET -dBATCH -sOutputFile=output.pdf input.pdf
/// ```
///
/// What we do though is to create an intermediate `.swp` file and then rename it to the expected one.
/// This is procedure of creating an intermediate file is a workaround to a limitation of the shell.
///
/// # Arguments
///
/// * `pdf_path` - The path to the PDF file to be optimized.
pub fn optimize_pdf_file_with_gs(pdf_path: &str) -> Result<(), ContextError> {
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
                ContextError::with_error("Unable to wait for the gs command execution", &error)
            })?;
            if !status.success() {
                return Err(ContextError::with_context(format!(
                    "gs failed with status {:?}",
                    status
                )));
            }
            std::fs::rename(format!("{}.swp", pdf_path), pdf_path).map_err(|error| {
                ContextError::with_error("Unable to rename the optimized PDF file", &error)
            })?;
        }
        Err(error) => {
            return Err(ContextError::with_error(
                "Unable to run the gs command",
                &error,
            ));
        }
    }

    Ok(())
}
