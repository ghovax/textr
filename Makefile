all: test

clean:
	cargo clean

docs:
	cargo doc

test:
	@for document_path in $(shell ls assets/*.json); do \
		RUST_LOG=$(RUST_LOG) cargo run -- --document $${document_path} ; \
	done