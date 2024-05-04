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
    WriteUnicodeText {
        /// The color of the text.
        color: [f32; 3],
        /// The position of the text.
        position: [f32; 2],
        /// The text to be rendered, save the in an UTF-8-compatible format.
        text_string: String,
        /// The font size of the text.
        font_size: f32,
        /// The font index of the text, used in order to retrieve the proper font.
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
        // Read the document content from the given path into a string
        let document_content = std::fs::read_to_string(document_path).map_err(|error| {
            ContextError::with_error(
                format!("Unable to read the document {:?}", document_path),
                &error,
            )
        })?;
        // Deserialize the document content into the `Document` struct
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
    pub fn to_pdf_document(&self) -> Result<PdfDocument, ContextError> {
        // Create a PDF document with the identifier of the document
        let mut pdf_document = PdfDocument::new(self.document_id.clone());

        // Load the built-in fonts present in the `fonts` directory of the CMU family
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
                                  // Sort the font paths in order to load them in the correct order
        font_paths.sort();
        // Load the math font as well
        let math_font_path = "fonts/lm-math/opentype/latinmodern-math.otf";
        font_paths.push(PathBuf::from_str(math_font_path).map_err(|error| {
            ContextError::with_error(
                format!("Failed to read the font file {:?}", math_font_path),
                &error,
            )
        })?);

        // Add the fonts to the document one after the other
        for font_path in font_paths {
            let _font_index = pdf_document.add_font(&font_path).unwrap();
        }

        // Currently the only states that this PDF-writing function is handling is the current index of the page and of the
        // layer in the page, which are needed to write the text to the layer in the page
        // Any user of this library would anyway still need to take care of the indices
        let mut current_page_index = 0;
        let mut current_layer_index_in_page = 0;

        // Iterate over the operations in the document in order to map them to the associated operation
        // Note that the operations are iterated over in the order they are present in the document,
        // which is important for the correctness of the PDF document
        //
        // Also, the mapping is one to one because the operations are mapped to the operations in the PDF document
        // For instance, the `AppendNewPage` operation is mapped to the `add_page_with_layer` function of the `PdfDocument`
        // struct and the operation `WriteUnicodeText` is mapped to the function `write_text_to_layer_in_page`
        for operation in self.operations.iter() {
            match operation {
                Operation::WriteUnicodeText {
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
        let pdf_document_bytes = self
            .to_pdf_document()?
            .save_to_bytes(self.instance_id.clone())?;
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
