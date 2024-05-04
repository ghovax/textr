use std::{io::Write as _, path::Path};
use textr::{
    error::ContextError,
    pdf::{self, PdfDocument},
};

fn main() {
    // Set the log level to trace to see the full output
    env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .init();

    // Create a new document with a predefined document ID
    let document_id = "QU2KK7yivMeRDnU8DodEQxnfqJAe4wZ2".to_string();
    let mut pdf_document = PdfDocument::new(document_id);
    // Add a page of 300 by 500 millimeters with an empty layer
    let (page_index, layer_index_in_page) = pdf_document.add_page_with_layer(300.0, 500.0);

    // Add a font to the document, in this case it is the bold italic font of the CMU family
    let font_path = Path::new("fonts/computer-modern/cmunbi.ttf");
    let font_index = pdf_document.add_font(font_path).unwrap();

    // Write some text to the current page at the current layer
    pdf_document
        .write_text_to_layer_in_page(
            page_index,
            layer_index_in_page,
            [0.0, 0.0, 0.0],
            "Hello, world!".into(),
            font_index,
            48.0,
            [50.0, 200.0],
        )
        .unwrap();

    // Because we are not working with a `Document`, but instead with a `PdfDocument` we need
    // to first save the PDF document to bytes and then to a file
    let instance_id = "DLjCAhuTD3cvaoQCJnMvkC0iNWEGEfyD".to_string();
    let pdf_document_bytes = pdf_document.save_to_bytes(instance_id.clone()).unwrap();

    let pdf_file_path = format!("assets/{}.pdf", instance_id);
    let mut pdf_file = std::fs::File::create(pdf_file_path.clone())
        .map_err(|error| ContextError::with_error("Failed to create the output file", &error))
        .unwrap();
    pdf_file
        .write_all(&pdf_document_bytes)
        .map_err(|error| ContextError::with_error("Failed to save the output file", &error))
        .unwrap();

    // Note that all documents tend to be heavy so they need to be post-processed to be further optimized
    pdf::optimize_pdf_file_with_gs(&pdf_file_path).unwrap();
}
