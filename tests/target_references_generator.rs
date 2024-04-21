#[cfg(test)]
mod tests {
    use std::io::BufWriter;

    use textr::traceable_error::TraceableError;

    #[test]
    fn generate_target_references_from_fuzz_targets() {
        let fuzz_targets = std::fs::read_dir("fuzz/fuzz_targets")
            .unwrap()
            .filter(|entry| {
                let entry = entry.as_ref().unwrap();
                entry.file_name().to_str().unwrap().ends_with(".json")
            });

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
            let pdf_document = textr::document::document_to_pdf(&document).unwrap();
            pdf_document
                .save(&mut BufWriter::new(
                    std::fs::File::create(format!(
                        "fuzz/target_references/{}.pdf",
                        fuzz_target_file_stem
                    ))
                    .unwrap(),
                ))
                .unwrap();
        }
    }
}
