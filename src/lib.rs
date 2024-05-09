//! TeXtr is an interface for the generation of PDF documents from the parsing of
//! a JSON document which adheres to a specific format compatible with the library.
//! This format is specified in the `Document` struct, which offers a method called `to_pdf`
//! that converts this high-level representation into a PDF document.
//!
//! In this crate, PDF documents are represented by the struct `PdfDocument`, which offers a high-level
//! interface for direct PDF manipulation. The nitty-gritty details for the manipulation of PDF documents
//! are hidden in the implementation of this struct, but in any case, if needed, they
//! are to a certain degree exposed to the end-user.

/// The module were the `Document` interface is presented.
///
/// # Introduction
///
/// The entry point of this module is the `Document` struct. The end user can construct one either from code
/// or from a well constructed JSON document which comprises on a document ID, an instance ID and all the
/// relevant operations for creating a PDF document which are so far compatible.
/// This structs acts as a intermediate representation of what a PDF document may comprise of, such as
/// text and its position, color, font and size, but also the possible presence of images. Although, this last feature
/// has not yet been implemented. For the supported operations see the `Operation` enum.
///
/// The main use an end user might have of this library is again as an intermediate
/// representation of a PDF document format, so that if algorithms are written that layout the text, or in general the contents,
/// this crate would take care of converting such representation into a properly formatted PDF document.
/// This is made possible thanks to the `to_pdf_document` (or either `save_to_pdf_file`) method of this struct, which will return a `PdfDocument`
/// if it is successfully able to convert the document into a PDF document representation, which can then be saved.
pub mod document;

/// This module contains the `ContextError` type which is the error type used throughout this library.
///
/// The reason why this type has been implemented is to uniform the error reporting without delving to deep
/// into specific error codes which for such library would be too many and definitely out of scope.
///
/// The `ContextError` type is always returns from a `Result` type, which means that the end user can expect to obtain an explanation
/// whenever a function returns an error. If an error happened in a function which was called inside a function of this library,
/// then the user can expect to also obtain information about this propagated error.
///
/// Also, the `ContextError` type implements `std::fmt::Display` and `Debug`, so it can be explicitly printed out. It is also
/// a public type, which means that it can be reused in different libraries by implementing functions or external traits on top of it.
pub mod error;

/// The module were the `PdfDocument` interface for working with PDF documents is presented.
///
/// # Disclaimer
///
/// This work was partially adapted from the one of [fschutt](https://github.com/fschutt) for the crate [printpdf](https://github.com/fschutt/printpdf).
/// The [specification for the PDF format](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)
/// was also briefly studied before committing to this project. The reason why the crate `printpdf`
/// could not be used in this project as such was because it employed the random generation of parameters
/// such as the PDF identifier and the the instance ID, which made the testing completely unpredictable.
///
/// Personally I have also found the endless PDF code insert by fschutt to be poorly documented, so I have taken this into
/// consideration and fixed this issue during my work by trying to add an extensive documentation suite.
///
/// The documents produced by this crate are loosely "correct" in the sense that they can be successfully
/// parsed by any PDF application so that their content is displayed as expected, but the PDFs need to be
/// run through either `gs` or `ps2pdf` so that the size is not only reduced but the documents are "cleaned up"
/// in their internal representation of the contents. For this reason I have included two auxiliary functions
/// which are `optimize_pdf_file_with_gs` and `optimize_pdf_file_with_ps2pdf`. These
/// functions rely on the pre-installed versions of `gs` and `ps2pdf` onto the operating system of the end user,
/// so it is to be noted that they are not cross-platform compatible.
///
/// # Introduction
///
/// The main component of this module is the struct `PdfDocument`. For it, I have implemented different convenience functions
/// such as `add_page_with_layer`, `add_font`, `write_text_to_layer_in_page`, `write_all` and `save_to_bytes` which allow the end user to interact
/// with a PDF document in a meaningful way, while keeping all the complexity hidden below a curtain of private methods.
pub mod pdf;
