BINSTALL := $(shell command -v cargo-binstall 2> /dev/null)

.PHONY: setup-binstall
setup-binstall:
ifndef BINSTALL
	@echo "Installing cargo-binstall..."
ifeq ($(OS),Windows_NT)
	@powershell -c "iex (irm https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.ps1)"
else
	@curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
endif
endif

.PHONY: setup
setup: setup-binstall
	@cargo binstall -y cargo-nextest cargo-edit cargo-sort cargo-machete cargo-tarpaulin

.PHONY: format
format:
	cargo +nightly fmt --all

.PHONY: check
check: format
	cargo +nightly clippy --all-features --all-targets --fix --allow-dirty -- -D warnings

.PHONY: test
test: check
	cargo nextest run --config-file nextest.toml $(filter-out $@,$(MAKECMDGOALS))

.PHONY: bench
bench: check
	cargo bench

.PHONY: upgrade
upgrade:
	cargo upgrade --incompatible && cargo sort -w

.PHONY: machete
machete:
	cargo machete --with-metadata

.PHONY: tarpaulin
tarpaulin:
	cargo tarpaulin --all-targets -- --test-threads 1 --no-capture
