format:
	cargo +nightly fmt --all

check: format
	cargo +nightly clippy --all-features --all-targets --fix --allow-dirty -- -D warnings

test: check
	cargo nextest run --config-file nextest.toml $(filter-out $@,$(MAKECMDGOALS))

bench: check
	cargo bench

upgrade:
	cargo upgrade --incompatible && cargo sort -w

machete:
	cargo machete --with-metadata

tarpaulin:
	cargo tarpaulin --all-targets -- --test-threads 1 --no-capture
