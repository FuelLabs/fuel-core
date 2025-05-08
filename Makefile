.PHONY: help
help:
	@echo "Supported make targets:"
	@echo "  make build        - Build release binary with production features"
	@echo "  make debug        - Build debug binary with production features"
	@echo "  make fmt          - Format all code"
	@echo "  make test         - Run all tests"
	@echo "  make ci-checks    - Run all CI checks"
	@echo "  make bench        - Run benchmarks"
	@echo "  make clean        - Clean build artifacts"

.PHONY: build
build:
	cargo build-fuel-core-bin-release

.PHONY: debug
debug:
	cargo build-fuel-core-bin

.PHONY: fmt
fmt:
	cargo +nightly fmt

.PHONY: test
test:
	cargo nextest run --all-targets --all-features

.PHONY: ci-checks
ci-checks:
	./ci_checks.sh

.PHONY: bench
bench:
	cargo bench -p fuel-core-benches

.PHONY: clean
clean:
	cargo clean 
