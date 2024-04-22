#[cfg(test)]
mod tests {
    const PDF_EQUALITY_THRESHOLD: f64 = 0.37;

    #[test]
    fn compare_fuzz_targets_with_reference_pdfs() {
        let fuzz_targets = std::fs::read_dir("fuzz/fuzz_targets")
            .unwrap()
            .filter(|entry| {
                let entry = entry.as_ref().unwrap();
                entry.file_name().to_str().unwrap().ends_with(".json")
            });

        for fuzz_target in fuzz_targets {
            let fuzz_target_path = fuzz_target.unwrap().path();
            let fuzz_target_file_stem = fuzz_target_path.file_stem().unwrap().to_str().unwrap();

            let document_content =
                std::fs::read(format!("fuzz/fuzz_targets/{}.json", fuzz_target_file_stem)).unwrap();
            let document: textr::document::Document =
                serde_json::from_slice(&document_content).unwrap();
            let pdf_document_bytes = textr::document::document_to_pdf(&document)
                .unwrap()
                .save_to_bytes()
                .unwrap();
            let pdf_document = lopdf::Document::load_mem(&pdf_document_bytes).unwrap();
            let pdf_document_id = pdf_document.trailer.get(b"ID").unwrap().clone();

            let other_pdf_document_bytes = std::fs::read(format!(
                "fuzz/target_references/{}.pdf",
                fuzz_target_file_stem
            ))
            .unwrap();
            let other_pdf_document = lopdf::Document::load_mem(&other_pdf_document_bytes).unwrap();
            let other_pdf_document_id = other_pdf_document.trailer.get(b"ID").unwrap().clone();

            assert_eq!(
                pdf_document_id, other_pdf_document_id,
                "the document {:?} does not have a matching ID with its counterpart, they are definitely different",
                fuzz_target_file_stem
            );
            assert_eq!(
                pdf_document_bytes.len(),
                other_pdf_document_bytes.len(),
                "the document {:?} differs in size from its counterpart: {} != {}, they may have been produced in different release modes or just be different files",
                fuzz_target_file_stem,
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

            assert!(
                byte_difference_percentage < PDF_EQUALITY_THRESHOLD,
                "the document {:?} differs in number of bytes from its counterpart by more than the accepted threshold: {} > {}, either change the threshold or accept that they are different files",
                fuzz_target_file_stem,
                byte_difference_percentage,
                PDF_EQUALITY_THRESHOLD
            );
        }
    }
}
