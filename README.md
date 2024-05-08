# TeXtr

TeXtr is an interface for the generation of PDF documents from the parsing of 
a JSON documentwhich adheres to a specific format compatible with the library. 
This format is specified in the Document struct, which offers a method `to_pdf` 
that converts this high-level format into a PDF document. In this crate, PDF 
documents are represented by the struct `PdfDocument`, which also offers a high-level 
interface for PDF manipulation. The nitty-gritty details of PDF documents manipulation 
are hidden in the implementation of this struct, but in any case, if needed, they 
are to a certain degree exposed to the end-user.

# Installation

Of course, after having installed Rust through [rustup](https://rustup.rs) or 
either way having updated Rust to the latest version, add the following to your 
`Cargo.toml` file in your project in order to use the library.
```toml
textr = "0.2.0"
```

If you want to try out this library yourself from the given examples, then clone 
this repository and then run the command:
```bash
cargo run --example <example_name>
```
where `example_name` can be one of the following, either `document_to_pdf` or
`pdf_from_scratch`. The example `document_to_pdf` is a command-line utility
which will allow you to easily convert any JSON document which adheres to the
format specific to this library (examples can be found in the `assets` folder).

# Testing

This library has been pseudo-fuzz tested in order to make sure that its output 
is both consistent under a wide variety of cases and works as expected. If you
want to run the tests personally, then all you need to do is to first generate the 
fuzz targets (JSON documents randomized in content but still adhering it to the right
format) by running the test function `generate_fuzz_targets` via the command:
```bash
cargo test generate_fuzz_targets -- --exact
```

This command will populate the `fuzz/fuzz_targets` directory with JSON files.
The next step is then the execution of the test function `generate_target_references_from_fuzz_targets` 
via the command which will generate the reference PDF documents, save them and then convert 
them to the postscript document format in the `fuzz/targets_references` folder. 
Just for reference, the postscript format is a textual equivalent to a PDF document
which allows for easier comparison because the PDF is a byte format.
This generation of the reference documents can be done via the command:
```bash
cargo test generate_target_references_from_fuzz_targets -- --exact
```

Once this command has finished executing, the newly generated reference documents
can be compared with the dynamically generated ones via the test routine
`compare_fuzz_targets_with_target_references` by employing the command:
```bash
cargo test compare_fuzz_targets_with_target_references -- --exact
```

This is especially useful for regression testing as one can verify that
the library still produces the same output after substantial changes in the code.

## Disclaimer

The test cases are not included with this library because they are quite heavy,
so instead what is done is I have included the generation code and personally
tested it on my machine after generating the reference documents. What is suggested
is that before any developing is done on this library, one generates the
reference documents by following the testing procedure and then verify as the code is modified
as that the output is still predictable and exactly the same as it was before.

# Comparison with other libraries

During the development of this library I have stumbled upon [pdf_writer](https://github.com/typst/pdf-writer)
which looks under any perspective superior to this library when it comes to the PDF-writing 
implementation. So while my library (as written in the documentation, inspired by and adapted from 
[printpdf](https://github.com/fschutt/printpdf)) covers certain needs, I think it is definitely an 
overshoot to provide its own PDF implementation. What I could do is provide the PDF writing backbone 
by employing the `pdf_writer` library, but because my library already is good at what it needs so far 
to do, I will not implement this change in the present time.

> Made in Rust with love 🦀