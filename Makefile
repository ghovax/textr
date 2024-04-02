EXE = cargo run -- --debug --config configs/default_config.json

# Visualize all the documents one by one sequentially
all:
	@for document_path in $(shell ls documents/*.json); do \
		$(EXE) --document $${document_path} ; \
	done

# Remove all of the files created during compilation, and also the binary
clean:
	cargo clean

# Build the documentation
docs:
	cargo doc

# Generate the reference images, setting the reference for the comparisons
generate:
	$(EXE) --test generate-reference-images

# Run the comparison between the dynamically generated images and the reference images
compare:
	$(EXE) --test compare-with-reference-images

install:
	cargo install --path .