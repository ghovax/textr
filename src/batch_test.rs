// #![deny(clippy::unwrap_used, clippy::expect_used)]

#[cfg(test)]
mod tests {
    use clap::{Parser, ValueEnum};
    use itertools::Itertools as _;
    use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator};
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    use crate::document::Document;
    use crate::document_configuration::DocumentConfiguration;
    use crate::image_system::{DocumentInterface as _, ImageSystem};
    use crate::traceable_error::{minimize_first_letter, TraceableError};

    #[derive(Debug, Copy, Clone, ValueEnum)]
    enum TestMode {
        GenerateImages,
        ValidateImages,
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    pub struct TestConfiguration {
        pub test_mode: String,
        pub use_debug_mode: bool,
        pub log_files_folder: String,
        pub test_setups: Vec<TestSetup>,
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
    pub struct TestSetup {
        pub document_configuration_file_path: PathBuf,
        pub document_path: PathBuf,
        pub reference_image_path: PathBuf,
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

    #[derive(Parser, Debug)]
    #[command(version, long_about = None)]
    struct TestCliArguments {
        #[arg(long = "test-configuration", value_name = "json_test_config_file")]
        test_configuration_file_path: PathBuf,
    }

    #[test]
    fn batch_test_from_configuration_file() {
        let test_arguments = TestCliArguments {
            test_configuration_file_path: "test_configs/batch_test_basic_config.json".into(),
        };

        let test_configuration =
            TestConfiguration::from_path(test_arguments.test_configuration_file_path);

        let failed_tests: Vec<_> = test_configuration
            .test_setups
            .par_iter()
            .filter_map(|test_setup| {
                if run_single_test(test_setup, &test_configuration).is_err() {
                    Some((
                        test_setup.document_path.clone(),
                        test_setup.document_configuration_file_path.clone(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        if !failed_tests.is_empty() {
            panic!("{} tests failed: {:?}", failed_tests.len(), failed_tests);
        }
    }

    fn run_single_test(
        test_setup: &TestSetup,
        test_configuration: &TestConfiguration,
    ) -> Result<(), TraceableError> {
        let document_configuration =
            DocumentConfiguration::from_path(&test_setup.document_configuration_file_path)?;
        log::debug!(
            "The loaded configuration to test is: {:?}",
            document_configuration
        );

        let document = Document::from_path(&test_setup.document_path)?;
        log::debug!("The loaded document to test is: {:?}", document);

        let mut image_system = ImageSystem {};
        let test_image = image_system.render_document(&document, &document_configuration)?;

        let document_file_name = test_setup.document_path
            .file_stem()
            .ok_or(TraceableError::with_context("Unable to get the file stem for the document".into()))?
            .to_str()
            .ok_or(TraceableError::with_context(format!(
                "Unable to convert the file name of the document {:?} to a string compatible with UTF-8",
                test_setup.document_path.display()
            )))?;

        let mut similarity_scores = Vec::new();
        let test_mode = test_configuration.test_mode.clone().try_into()?;
        match test_mode {
            TestMode::GenerateImages => {
                test_image
                    .save(&test_setup.reference_image_path)
                    .map_err(|error| {
                        TraceableError::with_source(
                            "Unable to save the reference image".into(),
                            error.into(),
                        )
                    })?;
            }
            TestMode::ValidateImages => {
                let reference_image = image::open(&test_setup.reference_image_path)
                    .map_err(|error| {
                        TraceableError::with_source(
                            format!(
                                "Unable to open the reference image for the document {:?}",
                                document_file_name
                            ),
                            error.into(),
                        )
                    })?
                    .into_rgba8();

                let comparison_results = image_compare::rgba_hybrid_compare(
                    &test_image,
                    &reference_image,
                )
                .map_err(|error| {
                    TraceableError::with_source("Unable to compare the images".into(), error.into())
                })?;
                similarity_scores.push((document_file_name, comparison_results.score));
            }
        }

        match test_mode {
            TestMode::GenerateImages => {
                log::info!(
                    "Generated reference image for the document {:?}",
                    document_file_name
                );
            }
            TestMode::ValidateImages => {
                #[allow(clippy::unwrap_used)]
                let failed_tests = similarity_scores
                    .iter()
                    .filter(|(_, similarity_score)| *similarity_score < 1.0)
                    .collect_vec();
                if failed_tests.is_empty() {
                    log::info!(
                        "Successfully compared the document {:?} with the reference image",
                        document_file_name
                    );
                } else {
                    return Err(TraceableError::with_context(format!(
                        "The document {:?} has failed the similarity test with the reference image",
                        failed_tests
                    )));
                }
            }
        }

        Ok(())
    }
}
