.PHONY: test
test:
	cargo fmt -- --check
	cargo-sort --check --workspace
	cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
	cargo test --locked --all-features --workspace

.PHONY: format
format:
	cargo fmt
	cargo-sort --workspace

.PHONY: installer
installer:
	cd installer && makensis windows.nsi
