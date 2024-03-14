# Variables to configure the program
DOCUMENT = assets/textTest.txt
FONT = fonts/cmunrm.ttf

ARGS = --document $(DOCUMENT) --font $(FONT)

# Targets for execution
all:
	cargo run -- $(ARGS)

flamegraph:
	cargo flamegraph -- $(ARGS)

clean:
	cargo clean