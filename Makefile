format:
	cargo +nightly fmt --all

check: format
	cargo +nightly clippy --all-features --all-targets --fix --allow-dirty -- -D warnings

test: check
	cargo nextest run --config-file nextest.toml

bench: check
	cargo bench

machete:
	cargo machete --with-metadata

tarpaulin:
	cargo tarpaulin --all-targets -- --test-threads 1 --no-capture
