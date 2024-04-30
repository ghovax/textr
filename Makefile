# Remove all of the files created during compilation, but also the binary
clean:
	cargo clean

# Build the documentation
documentation:
	cargo doc

# In order to use this command the user needs to type the following
# $ make 1=<path/to/file.pdf> optimize
optimize:
	ps2pdf $1 $1.swp ; mv $1.swp $1