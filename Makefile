all:
	RUST_LOG=info cargo run

clean:
	cargo clean

documentation:
	cargo doc