all:
	RUST_LOG=$(RUST_LOG) cargo run -- --config assets/configUnicode.json

clean:
	cargo clean

documentation:
	cargo doc

test:
	@for config_path in $(shell ls assets/*.json); do \
		echo "Loading the configuration file $${config_path}" && \
		RUST_LOG=$(RUST_LOG) cargo run -- --config $${config_path} ; \
	done