ENDPOINT ?= mainnet.eth.streamingfast.io:443
START_BLOCK ?= 16000000
STOP_BLOCK ?= +100

.PHONY: build
build:
	cargo build --target wasm32-unknown-unknown --release

.PHONY: run
run: build
	substreams run -e $(ENDPOINT) substreams.yaml map_blocks -s $(START_BLOCK) -t $(STOP_BLOCK)

.PHONY: pack
pack: build
	substreams pack substreams.yaml
