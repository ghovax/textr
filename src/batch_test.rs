#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use clap::{Parser, ValueEnum};
    use itertools::Itertools as _;
    use rand::distributions::Alphanumeric;
    use rand::seq::SliceRandom;
    use rand::Rng as _;
    use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    use crate::document::{render_document_to_image, Document, DocumentContent};
    use crate::document_configuration::DocumentConfiguration;
    use crate::fonts_configuration::{FontAssociation, FontsConfiguration};
    use crate::traceable_error::{minimize_first_letter, TraceableError};

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct TestConfiguration {
        pub use_debug_mode: bool,
        pub log_files_folder: String,
        pub document_configurations_folder: String,
        pub documents_files_folder: String,
        pub reference_images_folder: String,
    }

    impl TestConfiguration {
        pub fn from_path(test_configuration_file_path: PathBuf) -> Self {
            let test_configuration_file_contents =
                std::fs::read_to_string(test_configuration_file_path).unwrap_or_else(|error| {
                    panic!(
                        "failed to read the test configuration file: {}",
                        minimize_first_letter(error.to_string())
                    )
                });
            let test_configuration: TestConfiguration =
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
    fn batch_validation_from_configuration_file() {
        let test_configuration =
            TestConfiguration::from_path("test_configs/batch_test_basic_config.json".into());

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
                .collect_vec();
        let documents_files = std::fs::read_dir(&test_configuration.documents_files_folder)
            .unwrap_or_else(|error| {
                panic!(
                    "failed to read the documents files folder: {}",
                    minimize_first_letter(error.to_string())
                )
            })
            .map(|result| result.unwrap())
            .collect_vec();

        let mut similarity_scores = Vec::new();

        for document_configuration_file in document_configurations_files.iter() {
            let document_configuration =
                DocumentConfiguration::from_path(&document_configuration_file.path()).unwrap();

            let document_configuration_file_name = document_configuration_file
                .file_name()
                .into_string()
                .unwrap();

            for document_file in documents_files.iter() {
                let document = Document::from_path(&document_file.path()).unwrap();

                let document_file_name = document_file.file_name().into_string().unwrap();
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

                let reference_image = image::open(&reference_image_path).unwrap().into_rgba8();

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
        }

        let failed_tests = similarity_scores
            .iter()
            .filter(|(_, similarity_score)| *similarity_score < 1.0)
            .collect_vec();

        if !failed_tests.is_empty() {
            panic!("{} tests failed: {:?}", failed_tests.len(), failed_tests);
        }
    }

    fn generate_root_environment(
        recursion_depth: usize,
        font_associations: &Vec<FontAssociation>,
    ) -> DocumentContent {
        let mut random_number_generator = rand::thread_rng();

        if recursion_depth == 0 {
            return DocumentContent::UnicodeCharacters {
                text_string: random_number_generator
                    .sample_iter(&Alphanumeric)
                    .take(10)
                    .map(char::from)
                    .collect(),
            };
        }

        match random_number_generator.gen_range(0..3) {
            0 => {
                let font_association = font_associations
                    .choose(&mut random_number_generator)
                    .unwrap();
                let environment_contents = (0..random_number_generator.gen_range(1..4))
                    .map(|_| generate_root_environment(recursion_depth - 1, font_associations))
                    .collect();
                DocumentContent::Environment {
                    font_family: font_association.font_family.to_owned(),
                    environment_contents,
                }
            }
            1 => {
                let initial_caret_position = vec![
                    random_number_generator.gen_range(0.0..100.0),
                    random_number_generator.gen_range(0.0..100.0),
                ];
                let line_contents = (0..random_number_generator.gen_range(1..4))
                    .map(|_| generate_root_environment(recursion_depth - 1, font_associations))
                    .collect();
                DocumentContent::Line {
                    initial_caret_position,
                    line_contents,
                }
            }
            _ => DocumentContent::UnicodeCharacters {
                text_string: random_number_generator
                    .sample_iter(&Alphanumeric)
                    .take(10)
                    .map(char::from)
                    .collect(),
            },
        }
    }
}
