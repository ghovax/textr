use image::{Rgba, RgbaImage};
use rand::{distributions::Alphanumeric, Rng};
use serde::Serialize as _;
use std::{io::Write as _, ops::Range, str::FromStr as _};
use textr::error::ContextError;

/// The function which generates the fuzz targets (the JSON files to be fed to the
/// `generate_target_references_from_fuzz_targets` function). Because this function is only exposed
/// to the developer, for simplicity I have made it so that the main way to configure the generation of the fuzz targets
/// is to alter the parameters inside the function definition it in order to obtain the desired range.
#[test]
fn generate_fuzz_targets() {
    // The number of documents to generate
    let documents_to_generate = 7;
    // The range of font indices to pick from (this is chosen based on the fonts loaded)
    let font_indices_range = 0..30;
    // The maximum number of elements
    let maximum_number_of_elements = 190;
    // The maximum string length for the unicode text
    let maximum_string_length = 230;
    // The range of font sizes to use in the document
    let font_size_range = 39.0..65.0;
    // The range of page widths to use when creating a new page
    let page_width_range = 200.0..1300.0;
    // The range of page heights to use when creating a new page
    let page_height_range = 200.0..800.0;
    // The range of elements positions to choose from when positioning any element
    let elements_position_range = 0.0..600.0;

    // Generate the number of documents specified one by one and collect them into a vector of documents
    let documents: Vec<_> = (0..documents_to_generate)
        .map(|_| {
            let mut rng = rand::thread_rng();
            // Randomly generate both the document ID and the instance ID as 32 character long strings
            // accordingly to the PDF specification
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

            // The requirement for each document is to initially create a page, so we generate the first page
            // by setting it with random width and height in the given range and appending its onto the operations vector
            let page_width = rng.gen_range(page_width_range.clone());
            let page_height = rng.gen_range(page_height_range.clone());
            let first_page = textr::document::Operation::AppendNewPage {
                page_width,
                page_height,
            };
            operations.push(first_page);

            let number_of_elements = rng.gen_range(1..maximum_number_of_elements);
            for _ in 0..number_of_elements {
                // Generate a random operation for each of the randomly selected number of elements
                // within the specified range and append it onto the operations vector
                let randomly_generated_operation = random_operation(
                    &mut rng,
                    elements_position_range.clone(),
                    maximum_string_length,
                    font_size_range.clone(),
                    font_indices_range.clone(),
                    page_width_range.clone(),
                    page_height_range.clone(),
                );
                operations.push(randomly_generated_operation);
            }

            // Then return to document with the constructed operations
            textr::document::Document {
                document_id,
                instance_id,
                operations,
            }
        })
        .collect();

    // Save all the documents to JSON files to the predefined path for fuzz targets
    documents.into_iter().for_each(|document| {
        // Create a file at the location
        let document_path = format!("fuzz/fuzz_targets/{}.json", document.document_id);
        let mut document_file = std::fs::File::create(document_path).unwrap();
        // Serialize the document to a pretty-formatted JSON and write it to the file
        let mut content_buffer = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut serializer = serde_json::Serializer::with_formatter(&mut content_buffer, formatter);
        document.serialize(&mut serializer).unwrap();
        document_file.write_all(&content_buffer).unwrap();
    });
}

/// Returns a randomly generated operation with a predefined chance (can be pre-configured by altering
/// the function definition) with the given parameters for the different properties of the operations.
fn random_operation(
    rng: &mut rand::rngs::ThreadRng,
    elements_position_range: Range<f32>,
    maximum_string_length: usize,
    font_size_range: Range<f32>,
    font_indices_range: Range<usize>,
    page_width_range: Range<f32>,
    page_height_range: Range<f32>,
) -> textr::document::Operation {
    // This variable represents the chance of selecting an operation over the other
    let operation_chance = rng.gen_range(0..=100);
    match operation_chance {
        // With a predefined 70% chance the `WriteUnicodeText` operation is chosen
        0..=69 => {
            let color = [
                rng.gen_range(0.0..=1.0),
                rng.gen_range(0.0..=1.0),
                rng.gen_range(0.0..=1.0),
            ];
            let position = [
                rng.gen_range(elements_position_range.clone()),
                rng.gen_range(elements_position_range),
            ];
            let text_string = random_utf8_characters(rng, maximum_string_length);
            let font_size = rng.gen_range(font_size_range.clone());
            let font_index = rng.gen_range(font_indices_range.clone());
            textr::document::Operation::WriteUnicodeText {
                color,
                position,
                text_string,
                font_size,
                font_index,
            }
        }
        // With a predefined 30% chance the `WriteImage` operation is chosen
        70..=100 => {
            let page_width = rng.gen_range(page_width_range.clone());
            let page_height = rng.gen_range(page_height_range.clone());
            textr::document::Operation::AppendNewPage {
                page_width,
                page_height,
            }
        }
        // No other possible range should be left out, so this branch is technically unreachable
        _ => unreachable!(),
    }
}

/// Returns a randomly generated string with a length within the given range of the maximum string length.
fn random_utf8_characters(rng: &mut rand::rngs::ThreadRng, maximum_string_length: usize) -> String {
    let length = rng.gen_range(1..=maximum_string_length);
    rand_utf8::rand_utf8(rng, length).to_string()
}

/// Generates a random image within the given range of the parameters defined in its body.
#[test]
fn generate_random_image() {
    // The range of image width and height to let the random generator choose from
    let image_width_range = 1..150;
    let image_height_range = 1..150;

    let mut rng = rand::thread_rng();
    // Create a new image with random width and height within the given ranges
    let mut image = RgbaImage::new(
        rng.gen_range(image_width_range),
        rng.gen_range(image_height_range),
    );

    // Randomly put pixels into the image
    for (_, _, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([
            rng.gen_range(0..=255),
            rng.gen_range(0..=255),
            rng.gen_range(0..=255),
            rng.gen_range(0..=255),
        ]);
    }
    // Save the image to a file to the predefined path with a name 32 characters long
    // The length of the name is an arbitrary decision for possible uniqueness without to much strictness,
    // but it is not a requirement for the fuzz test to pass, also one could use UUIDs if really needed
    let image_name = rng
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(32)
        .collect::<String>();
    image.save(format!("images/{}.png", image_name)).unwrap();
}

/// This function generates the target references (the PDF documents which get then converted to postscript)
/// starting from the fuzz targets (the JSON files representing the documents). It reads the fuzz targets
/// documents from the predefined directory in the `fuzz` folder, outputting the postscript files in the
/// target references folder present in the same directory. This function also temporarily generates a PDF file
/// which gets replaced it with the associated file in postscript format, with the creation date removed.
///
/// # Disclaimer
///
/// In order to run this function it is needed to have on the computer a shell which has available
/// in the PATH environment the commands `sed` and `pdf2ps`.
#[test]
fn generate_target_references_from_fuzz_targets() {
    // Get a list of all the fuzz targets in the predefined folder
    let fuzz_targets = std::fs::read_dir("fuzz/fuzz_targets")
        .unwrap()
        .filter(|entry| {
            let entry = entry.as_ref().unwrap();
            entry.file_name().to_str().unwrap().ends_with(".json")
        });

    // Empty out the directory contents except the .gitignore, keeping the folder
    // This is done in order to remove the previous files which may be present
    let entries = std::fs::read_dir("fuzz/target_references").unwrap();

    // Iterate over the entries and remove each one
    for entry in entries {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();

        if file_type.is_file() && entry.file_name() != ".gitignore" {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }

    // For each fuzz target...
    for fuzz_target in fuzz_targets {
        // Get the file stem (the file name without extension) of the fuzz target
        let fuzz_target_path = fuzz_target.unwrap().path();
        let fuzz_target_file_stem = fuzz_target_path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Read the JSON document and parse it as a `Document`
        let document_content =
            std::fs::read(format!("fuzz/fuzz_targets/{}.json", fuzz_target_file_stem))
                .map_err(|error| {
                    ContextError::with_error(
                        format!("Failed to read JSON document {:?}", fuzz_target_file_stem),
                        &error,
                    )
                })
                .unwrap();
        let document: textr::document::Document = serde_json::from_slice(&document_content)
            .map_err(|error| {
                ContextError::with_error(
                    format!("Failed to parse JSON document {:?}", fuzz_target_file_stem),
                    &error,
                )
            })
            .unwrap();
        // Generate the PDF document from the document and save it to the predefined path
        let pdf_document_path = std::path::PathBuf::from_str(&format!(
            "fuzz/target_references/{}.pdf",
            fuzz_target_file_stem
        ))
        .unwrap();
        document.save_to_pdf_file(&pdf_document_path).unwrap();

        // Convert the PDF document to postscript
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
                ContextError::with_error(
                    format!(
                        "Failed to remove creation date from PS document {:?}",
                        ps_document_path
                    ),
                    &error,
                )
            })
            .unwrap();

        // Remove the leftover PDF file
        let command = std::process::Command::new("rm")
            .arg(pdf_document_path.clone())
            .spawn();
        command
            .unwrap()
            .wait()
            .map_err(|error| {
                ContextError::with_error(
                    format!("Failed to remove PDF document {:?}", pdf_document_path),
                    &error,
                )
            })
            .unwrap();

        // Remove the `ps-e` leftover file from running the command `pdf2ps`
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
                ContextError::with_error(
                    format!("Failed to remove PS-e document {:?}", ps_e_file_path),
                    &error,
                )
            })
            .unwrap();
    }
}

/// This function is responsible for verifying that the PDF documents dynamically-generated by the latest version
/// of the library actually match the expected reference targets which were previously created.
/// The testing is done by loading the JSON documents from the predefined path, parsing them and then generating the
/// associated PDF document, which is converted to postscript in order for it to be tested against the target references
/// (which are the postscript files generated by the `generate_target_references_from_fuzz_targets` function).
///
/// # Disclaimer
///
/// Just as the function `generate_target_references_from_fuzz_targets`, this function needs to be run in a shell
/// that has available in the PATH environment the commands `pdf2ps`, `bash`, `rm` and `sed`.
#[test]
fn compare_fuzz_targets_with_target_references() {
    // Get a list of all the fuzz targets in the predefined folder
    let fuzz_targets = std::fs::read_dir("fuzz/fuzz_targets")
        .unwrap()
        .filter(|entry| {
            let entry = entry.as_ref().unwrap();
            entry.file_name().to_str().unwrap().ends_with(".json")
        });

    // For each fuzz target...
    for fuzz_target in fuzz_targets {
        // Get the file stem (the file name without extension) of the fuzz target
        let fuzz_target_path = fuzz_target.unwrap().path();
        let fuzz_target_file_stem = fuzz_target_path.file_stem().unwrap().to_str().unwrap();

        // Parse the JSON document from the predefined path into a `Document`
        let document_path = format!("fuzz/fuzz_targets/{}.json", fuzz_target_file_stem);
        let document_content = std::fs::read(document_path.clone()).unwrap();
        let document: textr::document::Document =
            serde_json::from_slice(&document_content).unwrap();

        // Save the document to a PDF file in the same path where the fuzz targets are located for the sake of simplicity
        let pdf_document_path = std::path::PathBuf::from_str(&format!(
            "fuzz/fuzz_targets/{}.pdf",
            fuzz_target_file_stem
        ))
        .unwrap();
        document.save_to_pdf_file(&pdf_document_path).unwrap();
        // Convert the PDF document to postscript
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
                ContextError::with_error(
                    format!(
                        "Failed to remove creation date from PS document {:?}",
                        ps_document_path
                    ),
                    &error,
                )
            })
            .unwrap();
        // Load the document as a string, but this time from the postscript file
        let ps_document_path = format!("fuzz/fuzz_targets/{}.ps", fuzz_target_file_stem);
        let ps_document_content = std::fs::read_to_string(ps_document_path).unwrap();

        // And then load the reference document saved in the postscript format from the target references path
        let reference_ps_document_path =
            format!("fuzz/target_references/{}.ps", fuzz_target_file_stem);
        let reference_ps_document_content =
            std::fs::read_to_string(reference_ps_document_path.clone()).unwrap();

        // Run a comparison test between the contents of the two documents, reporting
        // any differences in the console by using a diffing algorithm
        similar_asserts::assert_eq!(ps_document_content, reference_ps_document_content);

        // If the comparison is deemed successful, then remove all the leftover files from the dynamical
        // generation of the PDF and postscript documents from the fuzz targets
        // This is done by invoking the shell command `bash -c rm`
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
                ContextError::with_error(
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

/// This function is a convenience function responsible for converting a PDF file to a postscript file.
/// It does so by invoking the `pdf2ps` command, which needs to be available in the PATH environment of the shell.
///
/// # Arguments
///
/// * `pdf_file_path` - The path to the PDF file that needs to be converted to a postscript file.
/// * `ps_file_path` - The path to the postscript file that will be created from the PDF file.
fn convert_pdf_file_to_ps(pdf_file_path: &str, ps_file_path: &str) -> Result<(), ContextError> {
    // Create the paths to the PDF and postscript files
    let pdf_document_path = std::path::PathBuf::from_str(pdf_file_path).map_err(|error| {
        ContextError::with_error(
            format!("Failed to create the PDF document path {:?}", pdf_file_path),
            &error,
        )
    })?;
    let ps_document_path = std::path::PathBuf::from_str(ps_file_path).map_err(|error| {
        ContextError::with_error(
            format!("Failed to create the PS document path {:?}", pdf_file_path),
            &error,
        )
    })?;

    // Convert the saved PDF file to a postscript file via the command `pdf2ps`
    let command = std::process::Command::new("pdf2ps")
        .arg(pdf_document_path.clone())
        .arg(ps_document_path.clone())
        .spawn();
    command.unwrap().wait().map_err(|error| {
        ContextError::with_error(
            format!("Failed to convert PDF to PS document {:?}", pdf_file_path),
            &error,
        )
    })?;

    Ok(())
}
