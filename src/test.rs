#[cfg(test)]
mod tests {
    use rand::Rng as _;
    use rand::{distributions::Alphanumeric, prelude::SliceRandom};
    use rayon::iter::{IntoParallelIterator as _, ParallelIterator as _};
    use serde::{Deserialize, Serialize};
    use std::io::Write as _;
    use std::path::PathBuf;

    use crate::{document::DocumentContent, traceable_error::TraceableError};

    #[test]
    fn test_fuzz_targets_with_reference_pdfs() {
        let document_content =
            std::fs::read("fuzz/fuzz_targets/7914944f-c768-40dd-9707-45cf1d72a7d8.json").unwrap();
        let document: crate::document::Document =
            serde_json::from_slice(&document_content).unwrap();
        let mut pdf_document_bytes = crate::document::document_to_pdf(&document)
            .unwrap()
            .save_to_bytes()
            .unwrap();
        let mut pdf_document = lopdf::Document::load_mem(&pdf_document_bytes).unwrap();
        pdf_document.prune_objects();
        let pdf_document_id = pdf_document.trailer.get(b"ID").unwrap().clone();
        pdf_document.save_to(&mut pdf_document_bytes).unwrap();

        let mut other_pdf_document_bytes =
            std::fs::read("fuzz/reference_pdfs/7914944f-c768-40dd-9707-45cf1d72a7d8.pdf").unwrap();
        let mut other_pdf_document = lopdf::Document::load_mem(&other_pdf_document_bytes).unwrap();
        other_pdf_document.prune_objects();
        let other_pdf_document_id = other_pdf_document.trailer.get(b"ID").unwrap().clone();
        other_pdf_document
            .save_to(&mut other_pdf_document_bytes)
            .unwrap();

        assert_eq!(pdf_document_id, other_pdf_document_id);
        assert_eq!(
            pdf_document_bytes.len(),
            other_pdf_document_bytes.len(),
            "{} != {}",
            pdf_document_bytes.len(),
            other_pdf_document_bytes.len()
        );
        let byte_difference = pdf_document_bytes
            .iter()
            .zip(other_pdf_document_bytes.iter())
            .fold(0, |byte_difference, (pdf_byte, other_pdf_byte)| {
                if pdf_byte != other_pdf_byte {
                    byte_difference + 1
                } else {
                    byte_difference
                }
            });
        let byte_difference_percentage =
            (byte_difference as f64) / (pdf_document_bytes.len() as f64);
        const EQUALITY_THRESHOLD: f64 = 0.05;

        assert!(
            byte_difference_percentage < EQUALITY_THRESHOLD,
            "{} > {}",
            byte_difference,
            EQUALITY_THRESHOLD
        );
    }

    //

    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct RandomizedDocumentGeneratorConfiguration {
        documents_to_generate: u32,
        output_directory: PathBuf,
        font_families_to_sample_from: Vec<String>,
        maximum_number_of_elements: usize,
        maximum_string_length: usize,
        font_size_range: [u32; 2],
        page_width_range: [f32; 2],
        page_height_range: [f32; 2],
        elements_position_range: [f32; 2],
        images_directory: PathBuf,
        include_images: bool,
    }

    impl RandomizedDocumentGeneratorConfiguration {
        pub fn from_path(document_path: &PathBuf) -> Result<Self, TraceableError> {
            let document_configuration_content =
                std::fs::read_to_string(document_path).map_err(|error| {
                    TraceableError::with_error(
                        format!(
                            "Unable to read the document generator configuration {:?}",
                            document_path
                        ),
                        &error,
                    )
                })?;
            let document_configuration: Self =
                serde_json::from_str(&document_configuration_content).map_err(|error| {
                    TraceableError::with_error(
                        format!(
                            "Unable to parse the document generator configuration {:?}",
                            document_path
                        ),
                        &error,
                    )
                })?;

            Ok(document_configuration)
        }
    }

    #[test]
    #[ignore]
    fn generate_randomized_json_documents() {
        let configuration = RandomizedDocumentGeneratorConfiguration::from_path(
            &"fuzz/document_generator_config.json".into(),
        )
        .unwrap();
        let pdf_document_id = uuid::Uuid::new_v4();

        let documents: Vec<_> = (0..configuration.documents_to_generate)
            .into_par_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();

                let page_width_range = match configuration.page_width_range {
                    [minimum, maximum] => minimum..maximum,
                };
                let page_height_range = match configuration.page_height_range {
                    [minimum, maximum] => minimum..maximum,
                };

                crate::document::Document {
                    id: pdf_document_id.to_string(),
                    title: random_utf8_characters(&mut rng, &configuration),
                    author: random_utf8_characters(&mut rng, &configuration),
                    date_in_unix_timestamp: rng.gen_range(0..=253402300799),
                    page_width: rng.gen_range(page_width_range),
                    page_height: rng.gen_range(page_height_range),
                    raw_contents: generate_raw_contents(&mut rng, &configuration),
                }
            })
            .collect();

        // Save all the documents to JSON files
        documents.into_par_iter().for_each(|document| {
            let document_path = configuration
                .output_directory
                .join(format!("{}.json", pdf_document_id));
            let mut document_file = std::fs::File::create(document_path).unwrap();
            // Serialize the document to JSON and write it
            let mut content_buffer = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
            let mut serializer =
                serde_json::Serializer::with_formatter(&mut content_buffer, formatter);
            document.serialize(&mut serializer).unwrap();
            document_file.write_all(&content_buffer).unwrap();
        });
    }

    fn random_utf8_characters(
        rng: &mut rand::rngs::ThreadRng,
        configuration: &RandomizedDocumentGeneratorConfiguration,
    ) -> String {
        // let length = rng.gen_range(1..=configuration.maximum_string_length);
        // rand_utf8::rand_utf8(rng, length).to_string()
        let length = rng.gen_range(1..=configuration.maximum_string_length);
        let mut string = rng
            .sample_iter(&Alphanumeric)
            .take(length)
            .collect::<Vec<_>>();
        string.shuffle(rng);
        String::from_utf8(string).unwrap()
    }

    fn generate_raw_contents(
        rng: &mut rand::rngs::ThreadRng,
        configuration: &RandomizedDocumentGeneratorConfiguration,
    ) -> Vec<crate::document::DocumentContent> {
        let number_of_elements = rng.gen_range(1..=configuration.maximum_number_of_elements);

        (0..number_of_elements)
            .into_par_iter()
            .map(|_| {
                let mut rng = rand::thread_rng();

                let position_range = match configuration.elements_position_range {
                    [minimum, maximum] => minimum..maximum,
                };
                let image_files = std::fs::read_dir(&configuration.images_directory)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect::<Vec<_>>();

                let mut content_type = rng.gen_range(0..=10);
                if !configuration.include_images {
                    content_type = rng.gen_range(0..=90);
                }
                match content_type {
                    0..=90 => {
                        let font_size_range = match configuration.font_size_range {
                            [minimum, maximum] => minimum..maximum,
                        };
                        let font_family = configuration
                            .font_families_to_sample_from
                            .choose(&mut rng)
                            .unwrap();
                        let font_size = rng.gen_range(font_size_range);
                        let is_url = rng.gen::<bool>();

                        DocumentContent::UnicodeText {
                            text: random_utf8_characters(&mut rng, configuration),
                            font_family: font_family.clone(),
                            font_size: font_size as f32,
                            position: [
                                rng.gen_range(position_range.clone()),
                                rng.gen_range(position_range.clone()),
                            ],
                            color: [
                                rng.gen_range(0.0..=1.0),
                                rng.gen_range(0.0..=1.0),
                                rng.gen_range(0.0..=1.0),
                            ],
                            url: match is_url {
                                true => Some(random_utf8_characters(&mut rng, configuration)),
                                false => None,
                            },
                            highlight_area: match is_url {
                                true => {
                                    let x = rng.gen_range(position_range.clone());
                                    let y = rng.gen_range(position_range.clone());
                                    Some([
                                        x,
                                        y,
                                        x + rng.gen_range(position_range.clone()),
                                        y + rng.gen_range(position_range),
                                    ])
                                }
                                false => None,
                            },
                        }
                    }
                    91..=100 => DocumentContent::Image {
                        image_path: image_files
                            .choose(&mut rng)
                            .unwrap()
                            .as_path()
                            .to_str()
                            .unwrap()
                            .into(),
                        position: [
                            rng.gen_range(position_range.clone()),
                            rng.gen_range(position_range),
                        ],
                        scale: [rng.gen_range(0.0..=1.0), rng.gen_range(0.0..=1.0)],
                    },
                    _ => unreachable!(),
                }
            })
            .collect()
    }
}
