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

.PHONY: ci
ci: fmt-check lint test gen-check ## Everything the Rust CI runs, in order

##@ Help

.PHONY: help
help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z_-]+:.*?##/ { printf "  %-14s %s\n", $$1, $$2 } /^##@/ { printf "\n%s\n", substr($$0, 5) }' $(MAKEFILE_LIST)
