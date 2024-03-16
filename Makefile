# Variables to configure the program
DOCUMENT = assets/textTest.txt
FONT = fonts/unifont-15.1.05.otf

ARGS = --document $(DOCUMENT) --font $(FONT)

# Targets for execution
all:
	cargo run -- $(ARGS)

flamegraph:
	cargo flamegraph -- $(ARGS)

clean:
	cargo clean