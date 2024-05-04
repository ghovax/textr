use clap::Parser as _;
use std::path::PathBuf;
use textr::{error::ContextError, pdf};

/// The command line arguments are the path of the JSON document and the
/// path of the output PDF file, feel free to add more depending on the need.
#[derive(clap::Parser)]
struct CliArguments {
    /// The path of the JSON document.
    #[arg(short = 'd', long = "document", value_name = "document_file")]
    document_path: PathBuf,
    /// The path of the output PDF file.
    #[arg(short = 'o', long = "output", value_name = "output_file")]
    output_pdf_path: PathBuf,
}

fn main() {
    // Parse the command line arguments
    let cli_arguments = CliArguments::parse();
    // Read the JSON document and parse it into a `Document`
    let document_content = std::fs::read(cli_arguments.document_path.clone())
        .map_err(|error| {
            ContextError::with_error(
                format!(
                    "Failed to read JSON document {:?}",
                    cli_arguments.document_path
                ),
                &error,
            )
        })
        .unwrap();
    let document: textr::document::Document = serde_json::from_slice(&document_content)
        .map_err(|error| {
            ContextError::with_error(
                format!(
                    "Failed to parse JSON document {:?}",
                    cli_arguments.document_path
                ),
                &error,
            )
        })
        .unwrap();

    // Save the document as a PDF file and optimize the result with ghostscript
    document
        .save_to_pdf_file(&cli_arguments.output_pdf_path)
        .unwrap();
    pdf::optimize_pdf_file_with_gs(cli_arguments.output_pdf_path.as_os_str().to_str().unwrap())
        .unwrap();
}
