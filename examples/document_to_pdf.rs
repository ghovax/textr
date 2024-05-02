use std::path::PathBuf;

use clap::Parser as _;
use textr::{document, error::ContextError};

#[derive(clap::Parser)]
struct CliArguments {
    #[arg(short = 'd', long = "document", value_name = "document_file")]
    document_path: PathBuf,
    #[arg(short = 'o', long = "output", value_name = "output_file")]
    output_path: PathBuf,
}

fn main() {
    let cli_arguments = CliArguments::parse();
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
    document
        .save_to_pdf_file(&cli_arguments.output_path)
        .unwrap();
    document::optimize_pdf_file_with_gs(cli_arguments.output_path.as_os_str().to_str().unwrap())
        .unwrap();
}
