CARGO_CHECK = cargo fmt && cargo clippy --fix --allow-dirty -- -D warnings

lint:
	$(CARGO_CHECK)

test: lint
	cargo test
