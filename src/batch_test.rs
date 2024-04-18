#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use clap::ValueEnum;
    use itertools::Itertools as _;
    use rand::distributions::Alphanumeric;
    use rand::prelude::*;
    use rand::seq::SliceRandom;
    use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator as _, ParallelIterator};
    use serde::{Deserialize, Serialize};

    use std::path::PathBuf;

    use crate::document::{render_document_to_image, Document, DocumentContent};
    use crate::document_configuration::DocumentConfiguration;
    use crate::fonts_configuration::FontsConfiguration;
    use crate::traceable_error::{minimize_first_letter, TraceableError};

    #[derive(Debug, Copy, Clone, ValueEnum)]
    enum TestMode {
        GenerateImages,
        ValidateImages,
    }

    impl std::convert::TryFrom<std::string::String> for TestMode {
        type Error = TraceableError;

        fn try_from(value: std::string::String) -> Result<Self, Self::Error> {
            match value.as_str() {
                "generateImages" => Ok(TestMode::GenerateImages),
                "validateImages" => Ok(TestMode::ValidateImages),
                _ => Err(TraceableError::with_context(format!(
                    "The test mode {:?} is not supported",
                    value
                ))),
            }
        }
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct ImageTestConfiguration {
        pub test_mode: String,
        pub use_debug_mode: bool,
        pub log_files_folder: String,
        pub document_configurations_folder: String,
        pub documents_files_folder: String,
        pub reference_images_folder: String,
    }

    impl ImageTestConfiguration {
        pub fn from_path(test_configuration_file_path: PathBuf) -> Self {
            let test_configuration_file_contents =
                std::fs::read_to_string(test_configuration_file_path).unwrap_or_else(|error| {
                    panic!(
                        "failed to read the test configuration file: {}",
                        minimize_first_letter(error.to_string())
                    )
                });
            let test_configuration: ImageTestConfiguration =
                serde_json::from_str(&test_configuration_file_contents).unwrap_or_else(|error| {
                    panic!(
                        "failed to parse the test configuration file: {}",
                        minimize_first_letter(error.to_string())
                    )
                });

            test_configuration
        }
    }

    #[test]
    fn batch_image_generation_or_validation_from_configuration_file() {
        let test_configuration = ImageTestConfiguration::from_path(
            "test_configs/batch_image_test_basic_config.json".into(),
        );

        let fonts_configuration =
            FontsConfiguration::from_path(&"fonts/default_fonts_config.json".into()).unwrap();

        let document_configurations_files =
            std::fs::read_dir(&test_configuration.document_configurations_folder)
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to read the document configurations folder: {}",
                        minimize_first_letter(error.to_string())
                    )
                })
                .map(|result| result.unwrap())
                .filter(|document_configuration_file| {
                    // Filter out all files which aren't in the json format
                    document_configuration_file.file_type().unwrap().is_file()
                        && match document_configuration_file.path().extension() {
                            Some(extension) => extension.to_str().unwrap() == "json",
                            None => false,
                        }
                })
                .collect_vec();
        let documents_files = std::fs::read_dir(&test_configuration.documents_files_folder)
            .unwrap_or_else(|error| {
                panic!(
                    "failed to read the documents files folder: {}",
                    minimize_first_letter(error.to_string())
                )
            })
            .map(|result| result.unwrap())
            .filter(|document_file| {
                // Filter out all files which aren't in the json format
                document_file.file_type().unwrap().is_file()
                    && match document_file.path().extension() {
                        Some(extension) => extension.to_str().unwrap() == "json",
                        None => false,
                    }
            })
            .collect_vec();

        if documents_files.is_empty() {
            panic!("no documents files found in the documents files folder");
        } else if document_configurations_files.is_empty() {
            panic!("no document configurations files found in the document configurations folder");
        }

        let mut similarity_scores = Vec::new();
        let test_mode = TestMode::try_from(test_configuration.test_mode.clone()).unwrap();

        for document_configuration_file in document_configurations_files.iter() {
            let document_configuration =
                DocumentConfiguration::from_path(&document_configuration_file.path()).unwrap();

            let document_configuration_file_path = document_configuration_file.path();
            let document_configuration_file_name = document_configuration_file_path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();

            for document_file in documents_files.iter() {
                let document = Document::from_path(&document_file.path()).unwrap();

                // Retrieve the document file name without its extension by deleting the last 5 characters
                let document_file_name = document_file.file_name().to_str().unwrap().to_string()
                    [..document_file.file_name().to_str().unwrap().len() - 5]
                    .to_string();
                let reference_image_path =
                    PathBuf::from(&test_configuration.reference_images_folder).join(format!(
                        "{}_{}.png",
                        document_file_name, document_configuration_file_name
                    ));

                let test_image = render_document_to_image(
                    &document,
                    &document_configuration,
                    &fonts_configuration,
                )
                .unwrap();

                match test_mode {
                    TestMode::ValidateImages => {
                        let reference_image =
                            image::open(&reference_image_path).unwrap().into_rgba8();

                        let comparison_results =
                            image_compare::rgba_hybrid_compare(&test_image, &reference_image)
                                .unwrap_or_else(|error| {
                                    panic!(
                                "failed to compare the test image with the reference image: {}",
                                minimize_first_letter(error.to_string())
                            )
                                });
                        similarity_scores.push((document_file_name, comparison_results.score));
                    }
                    TestMode::GenerateImages => {
                        test_image.save(&reference_image_path).unwrap();
                    }
                }
            }
        }

        match test_mode {
            TestMode::ValidateImages => {
                let failed_tests: Vec<_> = similarity_scores
                    .par_iter()
                    .filter(|(_, similarity_score)| *similarity_score < 1.0)
                    .cloned()
                    .collect();

                if !failed_tests.is_empty() {
                    panic!("{} tests failed: {:?}", failed_tests.len(), failed_tests);
                }
            }
            TestMode::GenerateImages => (),
        }
    }

    fn generate_line_contents(
        rng: &mut ThreadRng,
        current_depth: u32,
        test_configuration: &DocumentGenerationTestConfiguration,
    ) -> Vec<DocumentContent> {
        let mut line_contents = Vec::new();

        for _ in 0..rng.gen_range(1..=test_configuration.max_number_of_elements) {
            let document_content =
                if rng.gen::<bool>() || current_depth >= test_configuration.max_depth {
                    let string_length = rng.gen_range(1..=test_configuration.max_string_length);
                    // Add UnicodeCharacters
                    DocumentContent::UnicodeCharacters {
                        text_string: rand_utf8::rand_utf8(rng, string_length).to_string(),
                    }
                } else {
                    // Add Environment
                    DocumentContent::Environment {
                        font_family: test_configuration
                            .font_families
                            .choose(rng)
                            .unwrap()
                            .clone(),
                        environment_contents: generate_line_contents(
                            rng,
                            current_depth + 1,
                            test_configuration,
                        ),
                    }
                };
            line_contents.push(document_content);
        }

        line_contents
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct DocumentGenerationTestConfiguration {
        pub document_configuration_files_folder: String,
        pub configuration_files_to_generate: u32,
        pub page_width_range: Vec<f32>,
        pub page_height_range: Vec<f32>,
        pub font_size_range: Vec<u32>,
        pub global_magnification_range: Vec<f32>,
        pub font_families: Vec<String>,
        pub documents_to_generate: u32,
        pub documents_folder: String,
        pub max_environment_contents: usize,
        pub default_depth: u32,
        pub max_depth: u32,
        pub initial_caret_position_range: Vec<f32>,
        pub max_string_length: usize,
        pub max_number_of_elements: usize,
    }

    impl DocumentGenerationTestConfiguration {
        pub fn from_path(test_configuration_file_path: PathBuf) -> Self {
            let test_configuration_file_contents =
                std::fs::read_to_string(test_configuration_file_path).unwrap_or_else(|error| {
                    panic!(
                        "failed to read the test configuration file: {}",
                        minimize_first_letter(error.to_string())
                    )
                });
            let test_configuration: DocumentGenerationTestConfiguration =
                serde_json::from_str(&test_configuration_file_contents).unwrap_or_else(|error| {
                    panic!(
                        "failed to parse the test configuration file: {}",
                        minimize_first_letter(error.to_string())
                    )
                });

            test_configuration
        }
    }

    #[test]
    fn batch_document_generation_from_configuration_file() {
        let test_configuration = DocumentGenerationTestConfiguration::from_path(
            "test_configs/batch_document_generation_config.json".into(),
        );

        let documents: Vec<_> = (0..test_configuration.documents_to_generate)
            .into_par_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();

                let initial_caret_position_range =
                    match test_configuration.initial_caret_position_range[..] {
                        [x, y] => x..y,
                        _ => panic!("invalid initialCaretPositionRange"),
                    };
                let root_environment = DocumentContent::Environment {
                    font_family: test_configuration
                        .font_families
                        .choose(&mut rng)
                        .unwrap()
                        .clone(),
                    environment_contents: (0..rng
                        .gen_range(1..=test_configuration.max_environment_contents))
                        .map(|_| DocumentContent::Line {
                            line_contents: generate_line_contents(
                                &mut rng,
                                test_configuration.default_depth,
                                &test_configuration,
                            ),
                            initial_caret_position: vec![
                                rng.gen_range(initial_caret_position_range.clone()),
                                rng.gen_range(initial_caret_position_range.clone()),
                            ],
                        })
                        .collect(),
                };
                Document { root_environment }
            })
            .collect();

        documents.par_iter().for_each(|document| {
            // Assign a random name to the document that will be saved
            let rng = rand::thread_rng();
            let document_name = rng
                .sample_iter(&Alphanumeric)
                .take(10)
                .map(char::from)
                .collect::<String>();

            let document_path = PathBuf::from(&test_configuration.documents_folder)
                .join(format!("{}.json", document_name));

            // Save the document
            let mut serialization_buffer = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut serializer =
                serde_json::Serializer::with_formatter(&mut serialization_buffer, formatter);
            document.serialize(&mut serializer).unwrap();

            let document_string = String::from_utf8(serialization_buffer).unwrap();
            std::fs::write(document_path, document_string).unwrap();
        });

        let page_width_range = match test_configuration.page_width_range[..] {
            [x, y] => x as u32..y as u32,
            _ => panic!("invalid pageWidthRange"),
        };
        let page_height_range = match test_configuration.page_height_range[..] {
            [x, y] => x as u32..y as u32,
            _ => panic!("invalid pageHeightRange"),
        };
        let font_size_range = match test_configuration.font_size_range[..] {
            [x, y] => x..y,
            _ => panic!("invalid fontSizeRange"),
        };
        let global_magnification_range = match test_configuration.global_magnification_range[..] {
            [x, y] => x..y,
            _ => panic!("invalid globalMagnificationRange"),
        };

        let document_configurations: Vec<_> = (0..test_configuration
            .configuration_files_to_generate)
            .into_par_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();

                DocumentConfiguration {
                    page_width: rng.gen_range(page_width_range.clone()),
                    page_height: rng.gen_range(page_height_range.clone()),
                    font_size: rng.gen_range(font_size_range.clone()),
                    global_magnification: rng.gen_range(global_magnification_range.clone()),
                }
            })
            .collect();

        document_configurations
            .par_iter()
            .for_each(|document_configuration| {
                // Assign a random name to the document that will be saved
                let rng = rand::thread_rng();
                let document_configuration_name = rng
                    .sample_iter(&Alphanumeric)
                    .take(10)
                    .map(char::from)
                    .collect::<String>();

                let document_configuration_path =
                    PathBuf::from(&test_configuration.document_configuration_files_folder)
                        .join(format!("{}.json", document_configuration_name));

                // Save the document
                let mut serialization_buffer = Vec::new();
                let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
                let mut serializer =
                    serde_json::Serializer::with_formatter(&mut serialization_buffer, formatter);
                document_configuration.serialize(&mut serializer).unwrap();

                let document_string = String::from_utf8(serialization_buffer).unwrap();
                std::fs::write(document_configuration_path, document_string).unwrap();
            });
    }
}
