# 0.1.0

First version with API and documentation almost complete.

# 0.2.0

Documentation bump-up with slight modification to the `PdfDocument` API which isn't 
backwards-compatible: its function `save_to_bytes` is now split in `optimize`, `write_all`
and `save_to_bytes` which does only what it says and nothing else.

# 0.2.1

Updated the documentation to include a README.

# 0.2.2

Fixed some things in the README and a spelling mistake in the documentation.

# 0.3.0

Removed certain public fields throughout the `pdf.rs` file for clarity.