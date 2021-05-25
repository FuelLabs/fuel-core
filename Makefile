.PHONY: all interpreter

all:

interpreter: ## Compile the interpreter
	@RUSTFLAGS="-C target-cpu=native" \
			  cargo build --release \
			  --bin interpreter \
			  --features log,cli
	@mkdir -p build
	@cp target/release/interpreter build/

help: ## Display this help screen
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
