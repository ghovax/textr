# TeXtr

TeXtr is an interface for the generation of PDF documents from the parsing of 
a JSON document which adheres to a specific format compatible with the library. 
This format is specified in the `Document` struct, which offers a method called `to_pdf` 
that converts this high-level representation into a PDF document. 

In this crate, PDF documents are represented by the struct `PdfDocument`, which offers a high-level 
interface for direct PDF manipulation. The nitty-gritty details for the manipulation of PDF documents 
are hidden in the implementation of this struct, but in any case, if needed, they 
are to a certain degree exposed to the end-user.

# Documentation

The documentation is available on the website docs.rs at the following [link](https://docs.rs/textr/latest/textr/).

There is two ways to use this library: the first one is writing to a PDF document from 
scratch as shown in the `pdf_from_scratch` example, by calling functions of the struct `PdfDocument` such as 
`add_font`, insert `add_page_with_layer` and 
`write_text_to_layer_in_page`, followed by `write_all` which 
will finalize the document, that can then be saved to a file (this methodology requires both 
the document and instance IDs to be provided by the end-user of the library). 

The other way in which this library can be used (the intended one), is to 
provide either a JSON document which encodes the `Document` data structure or the 
data structure itself in code (possibly generated through some layouting algorithm) and 
then to call the function `Document::to_pdf`, which will automatically generate a 
`PdfDocument` that can be saved to a file or further manipulated.

# Installation

After having installed Rust through [rustup](https://rustup.rs) or 
either way having updated Rust to the latest version, add the following to your 
`Cargo.toml` file in your project in order to use the library.
```toml
textr = "0.2"
```

If you want to just try out this library yourself from the given examples, then clone 
this repository and run the command:
```bash
cargo run --example <example_name>
```
where `example_name` can be one of the following: either `document_to_pdf` or
`pdf_from_scratch`. The example `document_to_pdf` is a command-line utility
which will allow you to easily convert any JSON document that adheres to the
format specific to this library (examples can be found in the `assets` folder) into a PDF document,
while the `pdf_from_scratch` example will generate a PDF document directly, 
bypassing the need for a JSON file deserializable into a `Document` struct.

# Testing

This library has been pseudo-fuzz tested in order to make sure that its output 
is both consistent under a wide variety of cases and works as expected. If you
want to run the tests personally, then all you need to do is to first generate the 
fuzz targets (JSON documents randomized in content but still adhering to the right
format) by running the test function `generate_fuzz_targets` via the command:
```bash
cargo test generate_fuzz_targets -- --exact
```

This command will populate the `fuzz/fuzz_targets` directory with JSON files.
The next step is then the execution of the test function `generate_target_references_from_fuzz_targets` 
via the command which will generate the reference PDF documents, save them and then convert 
them to the postscript document format in the `fuzz/targets_references` folder. 

> Just for reference, the postscript format is a textual equivalent to a PDF document
which allows for easier comparison because PDFs are a byte format.

The generation of the reference documents can be done via the command:
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
so instead what is done is that I have included the generation code and personally
tested it on my machine after generating the reference documents. What is suggested
is that before any developing is done on this library, one generates the
reference documents by following the testing procedure and then verify as the code is modified
that the output is still predictable and exactly the same as it was before.

# Comparison with other libraries

During the development of this library I have stumbled upon [pdf_writer](https://github.com/typst/pdf-writer)
which looks under any perspective superior to this library when it comes to the PDF-writing 
implementation. So while my library (as written in the documentation, inspired by and adapted from 
[printpdf](https://github.com/fschutt/printpdf)) covers certain needs, I think it is definitely an 
overshoot to provide my own PDF implementation. 

What I could do is provide the PDF writing backbone 
by employing the `pdf_writer` library, but because my library already is good at what it needs so far 
to do, I will not implement this change at the present time.

> Made in Rust with love 🦀