# Configuration parameters for the user to tweak
WINDOW_WIDTH = 1600
WINDOW_HEIGHT = 900

# Compilation shortcuts, not to be fiddled with
SHARED_ARGS = --window-width $(WINDOW_WIDTH) --window-height $(WINDOW_HEIGHT)
RUN_COMMAND = RUST_LOG=$(RUST_LOG) cargo run --

all: run-all-sequentially

# Remove all of the files created during compilation, and also the binary
clean:
	cargo clean

# Build the documentation
documentation:
	cargo doc

# Visualize all the documents one by one sequentially
run-all-sequentially:
	@for document_path in $(shell ls assets/*.json); do \
		$(RUN_COMMAND) $(SHARED_ARGS) --document-path $${document_path} ; \
	done

# Generate the reference images, setting the reference for the comparisons
generate-reference-images:
	$(RUN_COMMAND) $(SHARED_ARGS) --test-flag generate-reference-images

# Run the comparison between the dynamically generated images and the reference images
compare-with-reference-images:
	$(RUN_COMMAND) $(SHARED_ARGS) --test-flag compare-with-reference-images