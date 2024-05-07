# 0.1.0

First version with API and documentation almost complete.

# 0.2.0

Documentation bump-up with slight modification to the `PdfDocument` API which isn't 
backwards-compatible: its function `save_to_bytes` is now split in `optimize`, `write_all`
and `save_to_bytes` which does only what it says and nothing else.