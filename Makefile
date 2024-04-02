CMD = cargo run -- --debug --config configs/default_config.json 

all: run-all-sequentially

# Remove all of the files created during compilation, and also the binary
clean:
	cargo clean

# Build the documentation
docs:
	cargo doc

# Visualize all the documents one by one sequentially
run-all-sequentially:
	@for document in $(shell ls documents/*.json); do \
		$(CMD) load --document $${document} ; \
	done

# Generate the reference images, setting the reference for the comparisons
generate:
	$(CMD) test generate

# Run the comparison between the dynamically generated images and the reference images
compare:
	$(CMD) test compare