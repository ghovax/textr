#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};
    use rand::prelude::SliceRandom;
    use rand::Rng as _;
    use serde::{Deserialize, Serialize};
    use std::io::Write as _;
    use std::path::PathBuf;

    use textr::{document::DocumentContent, traceable_error::TraceableError};
    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct FuzzTargetsGeneratorConfiguration {
        documents_to_generate: u32,
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

    impl FuzzTargetsGeneratorConfiguration {
        pub fn from_path(path: &PathBuf) -> Result<Self, TraceableError> {
            let configuration_content = std::fs::read_to_string(path).map_err(|error| {
                TraceableError::with_error(
                    format!(
                        "Unable to read the fuzz targets generator configuration {:?}",
                        path
                    ),
                    &error,
                )
            })?;
            let configuration: Self =
                serde_json::from_str(&configuration_content).map_err(|error| {
                    TraceableError::with_error(
                        format!(
                            "Unable to parse the fuzz targets generator configuration {:?}",
                            path
                        ),
                        &error,
                    )
                })?;

            Ok(configuration)
        }
    }

    #[test]
    fn generate_fuzz_targets_from_configuration_file() {
        let configuration = FuzzTargetsGeneratorConfiguration::from_path(
            &"tests/fuzz_targets_generator_configuration.json".into(),
        )
        .unwrap();

        let documents: Vec<_> = (0..configuration.documents_to_generate)
            .map(|_| {
                let mut rng = rand::thread_rng();
                let pdf_document_id = uuid::Uuid::new_v4();

                let page_width_range = match configuration.page_width_range {
                    [minimum, maximum] => minimum..maximum,
                };
                let page_height_range = match configuration.page_height_range {
                    [minimum, maximum] => minimum..maximum,
                };

                textr::document::Document {
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
        documents.into_iter().for_each(|document| {
            let document_path = format!("fuzz/fuzz_targets/{}.json", document.id);
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
        configuration: &FuzzTargetsGeneratorConfiguration,
    ) -> String {
        let length = rng.gen_range(1..=configuration.maximum_string_length);
        rand_utf8::rand_utf8(rng, length).to_string()
    }

    fn generate_raw_contents(
        rng: &mut rand::rngs::ThreadRng,
        configuration: &FuzzTargetsGeneratorConfiguration,
    ) -> Vec<textr::document::DocumentContent> {
        let number_of_elements = rng.gen_range(1..=configuration.maximum_number_of_elements);

        (0..number_of_elements)
            .map(|_| {
                let mut rng = rand::thread_rng();

                let position_range = match configuration.elements_position_range {
                    [minimum, maximum] => minimum..maximum,
                };
                let image_files = std::fs::read_dir(&configuration.images_directory)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| match path.extension() {
                        Some(extension) => match extension.to_str() {
                            Some(extension) => extension == "png",
                            None => false,
                        },
                        None => false,
                    })
                    .collect::<Vec<_>>();

                let mut content_type = rng.gen_range(0..=100);
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
        image
            .save(format!("images/{}.png", uuid::Uuid::new_v4()))
            .unwrap();
    }
}
