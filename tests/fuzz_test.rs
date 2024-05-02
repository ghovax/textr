use std::{io::Write as _, ops::Range, str::FromStr as _};

use image::{Rgba, RgbaImage};
use rand::{distributions::Alphanumeric, Rng};
use serde::Serialize as _;
use textr::error::TraceableError;

struct FuzzTargetsGeneratorConfiguration {
    documents_to_generate: u32,
    font_indices_range: Range<usize>,
    maximum_number_of_elements: usize,
    maximum_string_length: usize,
    font_size_range: Range<f32>,
    page_width_range: Range<f32>,
    page_height_range: Range<f32>,
    element_position_range: Range<f32>,
}

#[test]
fn generate_fuzz_targets_from_configuration_file() {
    let configuration = FuzzTargetsGeneratorConfiguration {
        documents_to_generate: 30,
        font_indices_range: 0..30,
        maximum_number_of_elements: 190,
        maximum_string_length: 230,
        font_size_range: 39.0..65.0,
        page_width_range: 200.0..1300.0,
        page_height_range: 200.0..800.0,
        element_position_range: 0.0..600.0,
    };

    let documents: Vec<_> = (0..configuration.documents_to_generate)
        .map(|_| {
            let mut rng = rand::thread_rng();
            let document_id = rng
                .clone()
                .sample_iter(&Alphanumeric)
                .map(char::from)
                .take(32)
                .collect::<String>();
            let instance_id = rng
                .clone()
                .sample_iter(&Alphanumeric)
                .map(char::from)
                .take(32)
                .collect::<String>();

            let mut operations = Vec::new();

            let page_width = rng.gen_range(configuration.page_width_range.clone());
            let page_height = rng.gen_range(configuration.page_height_range.clone());
            let first_page = textr::document::Operation::AppendNewPage {
                page_width,
                page_height,
            };
            operations.push(first_page);

            for _ in 0..rng.gen_range(1..configuration.maximum_number_of_elements) {
                operations.push(random_operation(&mut rng, &configuration));
            }

            textr::document::Document {
                document_id,
                instance_id,
                operations,
            }
        })
        .collect();

    // Save all the documents to JSON files
    documents.into_iter().for_each(|document| {
        let document_path = format!("fuzz/fuzz_targets/{}.json", document.document_id);
        let mut document_file = std::fs::File::create(document_path).unwrap();
        // Serialize the document to JSON and write it
        let mut content_buffer = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut serializer = serde_json::Serializer::with_formatter(&mut content_buffer, formatter);
        document.serialize(&mut serializer).unwrap();
        document_file.write_all(&content_buffer).unwrap();
    });
}

fn random_operation(
    rng: &mut rand::rngs::ThreadRng,
    configuration: &FuzzTargetsGeneratorConfiguration,
) -> textr::document::Operation {
    let content_type_selector = rng.gen_range(0..=100);
    match content_type_selector {
        0..=69 => {
            let color = [
                rng.gen_range(0.0..=1.0),
                rng.gen_range(0.0..=1.0),
                rng.gen_range(0.0..=1.0),
            ];
            let position = [
                rng.gen_range(configuration.element_position_range.clone()),
                rng.gen_range(configuration.element_position_range.clone()),
            ];
            let text_string = random_utf8_characters(rng, configuration);
            let font_size = rng.gen_range(configuration.font_size_range.clone());
            let font_index = rng.gen_range(configuration.font_indices_range.clone());
            textr::document::Operation::UnicodeText {
                color,
                position,
                text_string,
                font_size,
                font_index,
            }
        }
        70..=100 => {
            let page_width = rng.gen_range(configuration.page_width_range.clone());
            let page_height = rng.gen_range(configuration.page_height_range.clone());
            textr::document::Operation::AppendNewPage {
                page_width,
                page_height,
            }
        }
        _ => unreachable!(),
    }
}

fn random_utf8_characters(
    rng: &mut rand::rngs::ThreadRng,
    configuration: &FuzzTargetsGeneratorConfiguration,
) -> String {
    let length = rng.gen_range(1..=configuration.maximum_string_length);
    rand_utf8::rand_utf8(rng, length).to_string()
}

#[test]
fn generate_random_image() {
    let mut rng = rand::thread_rng();
    let mut image = RgbaImage::new(rng.gen_range(1..=150), rng.gen_range(1..=150));
    for (_, _, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([
            rng.gen_range(0..=255),
            rng.gen_range(0..=255),
            rng.gen_range(0..=255),
            rng.gen_range(0..=255),
        ]);
    }
    let image_name = rng
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(32)
        .collect::<String>();
    image.save(format!("images/{}.png", image_name)).unwrap();
}

#[test]
fn generate_target_references_from_fuzz_targets() {
    let fuzz_targets = std::fs::read_dir("fuzz/fuzz_targets")
        .unwrap()
        .filter(|entry| {
            let entry = entry.as_ref().unwrap();
            entry.file_name().to_str().unwrap().ends_with(".json")
        });

    // Empty out the directory contents except the .gitignore, keeping the folder
    let entries = std::fs::read_dir("fuzz/target_references").unwrap();

    // Iterate over the entries and remove each one
    for entry in entries {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();

        // Check if the entry is a file or a directory
        if file_type.is_file() && entry.file_name() != ".gitignore" {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
    for fuzz_target in fuzz_targets {
        let fuzz_target_path = fuzz_target.unwrap().path();
        let fuzz_target_file_stem = fuzz_target_path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let document_content =
            std::fs::read(format!("fuzz/fuzz_targets/{}.json", fuzz_target_file_stem))
                .map_err(|error| {
                    TraceableError::with_error(
                        format!("Failed to read JSON document {:?}", fuzz_target_file_stem),
                        &error,
                    )
                })
                .unwrap();
        let document: textr::document::Document = serde_json::from_slice(&document_content)
            .map_err(|error| {
                TraceableError::with_error(
                    format!("Failed to parse JSON document {:?}", fuzz_target_file_stem),
                    &error,
                )
            })
            .unwrap();
        let pdf_document_path = std::path::PathBuf::from_str(&format!(
            "fuzz/target_references/{}.pdf",
            fuzz_target_file_stem
        ))
        .unwrap();
        document.save_to_pdf_file(&pdf_document_path).unwrap();

        let ps_document_path = std::path::PathBuf::from_str(&format!(
            "fuzz/target_references/{}.ps",
            fuzz_target_file_stem
        ))
        .unwrap();
        convert_pdf_file_to_ps(
            pdf_document_path.to_str().unwrap(),
            ps_document_path.to_str().unwrap(),
        )
        .unwrap();

        // Remove the creation date from the postscript file by using the `sed -i -e '7d' file.ps` command
        let command = std::process::Command::new("sed")
            .arg("-i")
            .arg("-e")
            .arg("7d")
            .arg(ps_document_path.clone())
            .spawn();
        command
            .unwrap()
            .wait()
            .map_err(|error| {
                TraceableError::with_error(
                    format!(
                        "Failed to remove creation date from PS document {:?}",
                        ps_document_path
                    ),
                    &error,
                )
            })
            .unwrap();

        // Remove all leftover PDF files
        let command = std::process::Command::new("rm")
            .arg(pdf_document_path.clone())
            .spawn();
        command
            .unwrap()
            .wait()
            .map_err(|error| {
                TraceableError::with_error(
                    format!("Failed to remove PDF document {:?}", pdf_document_path),
                    &error,
                )
            })
            .unwrap();

        let ps_e_file_path = std::path::PathBuf::from_str(&format!(
            "fuzz/target_references/{}.ps-e",
            fuzz_target_file_stem
        ))
        .unwrap();
        let command = std::process::Command::new("rm")
            .arg(ps_e_file_path.clone())
            .spawn();
        command
            .unwrap()
            .wait()
            .map_err(|error| {
                TraceableError::with_error(
                    format!("Failed to remove PS-e document {:?}", ps_e_file_path),
                    &error,
                )
            })
            .unwrap();
    }
}

#[test]
fn compare_fuzz_targets_with_target_references() {
    let fuzz_targets = std::fs::read_dir("fuzz/fuzz_targets")
        .unwrap()
        .filter(|entry| {
            let entry = entry.as_ref().unwrap();
            entry.file_name().to_str().unwrap().ends_with(".json")
        });

    for fuzz_target in fuzz_targets {
        let fuzz_target_path = fuzz_target.unwrap().path();
        let fuzz_target_file_stem = fuzz_target_path.file_stem().unwrap().to_str().unwrap();

        // Load the document from the JSON file
        let document_path = format!("fuzz/fuzz_targets/{}.json", fuzz_target_file_stem);
        let document_content = std::fs::read(document_path.clone()).unwrap();
        let document: textr::document::Document =
            serde_json::from_slice(&document_content).unwrap();

        // Save the document to a PDF file
        let pdf_document_path = std::path::PathBuf::from_str(&format!(
            "fuzz/fuzz_targets/{}.pdf",
            fuzz_target_file_stem
        ))
        .unwrap();
        document.save_to_pdf_file(&pdf_document_path).unwrap();
        let ps_document_path = std::path::PathBuf::from_str(&format!(
            "fuzz/fuzz_targets/{}.ps",
            fuzz_target_file_stem
        ))
        .unwrap();
        convert_pdf_file_to_ps(
            pdf_document_path.to_str().unwrap(),
            ps_document_path.to_str().unwrap(),
        )
        .unwrap();

        // Remove the creation date from the postscript file by using the `sed -i -e '7d' file.ps` command
        let command = std::process::Command::new("sed")
            .arg("-i")
            .arg("-e")
            .arg("7d")
            .arg(ps_document_path.clone())
            .spawn();
        command
            .unwrap()
            .wait()
            .map_err(|error| {
                TraceableError::with_error(
                    format!(
                        "Failed to remove creation date from PS document {:?}",
                        ps_document_path
                    ),
                    &error,
                )
            })
            .unwrap();
        // Load the PDF document from the PDF file as bytes
        let ps_document_path = format!("fuzz/fuzz_targets/{}.ps", fuzz_target_file_stem);
        let ps_document_string = std::fs::read_to_string(ps_document_path).unwrap();

        let other_ps_document_path = format!("fuzz/target_references/{}.ps", fuzz_target_file_stem);
        let other_ps_document_string =
            std::fs::read_to_string(other_ps_document_path.clone()).unwrap();

        similar_asserts::assert_eq!(ps_document_string, other_ps_document_string);

        let all_files_path = std::path::PathBuf::from_str(&format!(
            "fuzz/fuzz_targets/{}.pdf fuzz/fuzz_targets/{}.ps fuzz/fuzz_targets/{}.ps-e",
            fuzz_target_file_stem, fuzz_target_file_stem, fuzz_target_file_stem
        ))
        .unwrap();
        let command = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!("rm {}", all_files_path.to_str().unwrap()))
            .spawn();
        command
            .unwrap()
            .wait()
            .map_err(|error| {
                TraceableError::with_error(
                    format!(
                        "Failed to remove all documents for comparison {:?}",
                        all_files_path
                    ),
                    &error,
                )
            })
            .unwrap();
    }
}

fn convert_pdf_file_to_ps(pdf_file_path: &str, ps_file_path: &str) -> Result<(), TraceableError> {
    let pdf_document_path = std::path::PathBuf::from_str(pdf_file_path).map_err(|error| {
        TraceableError::with_error(
            format!("Failed to create the PDF document path {:?}", pdf_file_path),
            &error,
        )
    })?;
    let ps_document_path = std::path::PathBuf::from_str(ps_file_path).map_err(|error| {
        TraceableError::with_error(
            format!("Failed to create the PS document path {:?}", pdf_file_path),
            &error,
        )
    })?;

    // Convert the saved PDF file to a postscript file via the command pdf2ps in order to remove the creation date
    let command = std::process::Command::new("pdf2ps")
        .arg(pdf_document_path.clone())
        .arg(ps_document_path.clone())
        .spawn();
    command.unwrap().wait().map_err(|error| {
        TraceableError::with_error(
            format!("Failed to convert PDF to PS document {:?}", pdf_file_path),
            &error,
        )
    })?;

    Ok(())
}
