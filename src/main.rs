#![deny(clippy::unwrap_used, clippy::expect_used)]

use clap::Parser;
use std::path::PathBuf;
use traceable_error::TraceableError;

use crate::document::{render_document_to_image, Document};
use crate::document_configuration::DocumentConfiguration;
use crate::fonts_configuration::FontsConfiguration;

mod batch_test;
mod document;
mod document_configuration;
mod fonts_configuration;
mod traceable_error;

#[derive(Parser, Debug)]
#[command(version, long_about = None)]
struct CliArguments {
    #[arg(long = "document", value_name = "json_file")]
    document_path: PathBuf,
    #[arg(long = "document-configuration", value_name = "json_config_file")]
    document_configuration_file_path: PathBuf,
    #[arg(long = "fonts-configuration", value_name = "json_font_config_file")]
    fonts_configuration_file_path: PathBuf,
    #[arg(long = "debug", value_name = "bool", action = clap::ArgAction::SetTrue, default_value_t = false)]
    use_debug_mode: bool,
    #[arg(long = "output-image", value_enum, value_name = "image_path")]
    output_image_path: PathBuf,
}

fn main() {
    if let Err(error) = fallible_main() {
        log::error!("{}", error);
        std::process::exit(1);
    }
}

fn fallible_main() -> Result<(), TraceableError> {
    let arguments = CliArguments::parse();
    if arguments.use_debug_mode {
        env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();
    }
    log::debug!(
        "The program has been initialized with the parameters: {:?}",
        arguments
    );

    let fonts_configuration = FontsConfiguration::from_path(
        &arguments.fonts_configuration_file_path,
    )?;
    log::debug!("The loaded font configuration is: {:?}", fonts_configuration);

    let document_configuration =
        DocumentConfiguration::from_path(&arguments.document_configuration_file_path)?;
    log::debug!("The loaded configuration is: {:?}", document_configuration);

    let document = Document::from_path(&arguments.document_path)?;
    log::debug!("The loaded document is: {:?}", document);

    let image = render_document_to_image(&document, &document_configuration, &fonts_configuration)
        .map_err(|error| {
            TraceableError::with_source("Failed to render the document".into(), error.into())
        })?;

    image.save(arguments.output_image_path).map_err(|error| {
        TraceableError::with_source("Failed to save the rendered image".into(), error.into())
    })?;

    Ok(())
}
