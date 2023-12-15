ENDPOINT ?= mainnet.near.streamingfast.io:443
START_BLOCK ?= 96764162
STOP_BLOCK ?= +100

.PHONY: build
build:
	cargo build --target wasm32-unknown-unknown --release

.PHONY: run
run: build
	substreams run -e $(ENDPOINT) substreams.yaml near_filter -s $(START_BLOCK) -t $(STOP_BLOCK) $(ARGS)

.PHONY: gui
gui: build
	substreams gui -e $(ENDPOINT) substreams.yaml map_block -s $(START_BLOCK) -t $(STOP_BLOCK)

# .PHONY: protogen
# protogen:
# 	substreams protogen ./substreams.yaml --exclude-paths="sf/substreams,google"

.PHONY: pack
pack: build
	substreams pack substreams.yaml

.PHONY: deploy_local
deploy_local: pack
	mkdir build 2> /dev/null || true
	bun x graph build --ipfs http://localhost:5001 subgraph.yaml
	bun x graph create map_block --node http://127.0.0.1:8020
	bun x graph deploy --node http://127.0.0.1:8020 --ipfs http://127.0.0.1:5001 --version-label v0.0.1 map_block subgraph.yaml

.PHONY: undeploy_local
undeploy_local:
	graphman --config "$(GRAPH_CONFIG)" drop --force uniswap_v3

