use clap::Parser;
use std::{io::BufWriter, path::PathBuf};
use textr::{document::Document, traceable_error::TraceableError};

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct CliArguments {
    #[arg(short = 'd', long = "document", value_name = "json_file")]
    document_path: PathBuf,
    #[arg(short = 'o', long = "output", value_name = "file_path")]
    output_file_path: PathBuf,
}

fn main() {
    if let Err(error) = fallible_main() {
        log::error!("{}", error);
        std::process::exit(1);
    }
}

fn fallible_main() -> Result<(), TraceableError> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .init();
    let arguments = CliArguments::parse();
    log::debug!("{:?}", arguments);

    let document = Document::from_path(&arguments.document_path)?;
    log::debug!("{:?}", document);
    let pdf_document = textr::document::document_to_pdf(&document)
        .map_err(|error| TraceableError::with_error("Failed to render the document", &error))?;
    pdf_document
        .save(&mut BufWriter::new(
            std::fs::File::create(&arguments.output_file_path).map_err(|error| {
                TraceableError::with_error("Failed to create the output file", &error)
            })?,
        ))
        .map_err(|error| TraceableError::with_error("Failed to save the output file", &error))?;
    log::info!(
        "Saved the output file to the path: {:?}",
        arguments.output_file_path
    );
    Ok(())
}
