# Remove all of the files created during compilation, but also the binary
clean:
	cargo clean

# Build the documentation
documentation:
	cargo doc