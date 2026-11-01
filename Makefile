.DEFAULT_GOAL := help

##@ Build

.PHONY: build
build: ## Build the whole workspace
	cargo build --workspace

.PHONY: gen
gen: ## Regenerate the schema-derived sources
	cargo xtask gen

##@ Checks

.PHONY: fmt
fmt: ## Format all crates
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Fail on unformatted code, changing nothing
	cargo fmt --all -- --check

.PHONY: lint
lint: ## Clippy with warnings denied
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: test
test: ## Run the workspace tests
	cargo test --workspace

.PHONY: gen-check
gen-check: ## Fail when generated sources are stale
	cargo xtask gen --check

.PHONY: coverage-check
coverage-check: ## Fail when the schema coverage report is stale
	cargo xtask coverage --check

.PHONY: ci
ci: fmt-check lint test gen-check coverage-check ## Everything the Rust CI runs, in order

.PHONY: conformance
conformance: ## Write X8x files from the official fixtures and validate them against the official XSDs (needs uv)
	scripts/conformance.sh

.PHONY: corpus
corpus: ## Per-stage throughput and warnings over GAEBBOSS_CORPUS
	cargo xtask corpus

.PHONY: bench
bench: ## criterion benchmarks
	cargo bench -p gaebboss-core --bench lex --bench parse --bench items --bench write

##@ Python

.PHONY: develop
develop: ## Build and install the extension into .venv
	test -d .venv || uv venv --python 3.12 .venv
	uv pip install --python .venv/bin/python maturin pytest
	VIRTUAL_ENV=$(CURDIR)/.venv .venv/bin/maturin develop --uv --release

.PHONY: test-py
test-py: ## Run the Python tests against the installed extension
	.venv/bin/python -m pytest tests/python -q

.PHONY: bench-py
bench-py: ## Time gaebboss against pygaeb and lxml over GAEBBOSS_CORPUS
	uv pip install --python .venv/bin/python pygaeb lxml
	.venv/bin/python -I benchmarks/bench.py "$(GAEBBOSS_CORPUS)"

##@ Docs

.PHONY: book
book: ## Build the mdBook into docs/book
	mdbook build docs

##@ Help

.PHONY: help
help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z_-]+:.*?##/ { printf "  %-14s %s\n", $$1, $$2 } /^##@/ { printf "\n%s\n", substr($$0, 5) }' $(MAKEFILE_LIST)
