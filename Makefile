# Visualize all the documents one by one sequentially
all:
	@for document_path in $(shell ls documents/*.json); do \
		cargo run --debug --config document_configs/default_config.json -- --document $${document_path} ; \
	done

# Remove all of the files created during compilation, and also the binary
clean:
	cargo clean

# Build the documentation
docs:
	cargo doc

install:
	cargo install --path .

test:
	cargo test --test batch_test -- --test-config test_configs/test_basic_config.json