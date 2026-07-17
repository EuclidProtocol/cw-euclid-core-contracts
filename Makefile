.PHONY: compile
# The optimizer container mounts only this dir as /code; the repo .git lives at
# the repo root and is NOT visible inside it, so euclid's build.rs cannot read
# git. Capture commit + commit-time on the host and pass them in as env vars.
compile:
	eval "$$(sh ../../scripts/build-info.sh)" && \
	docker run --rm -v .:/code \
		--env BUILD_COMMIT="$$BUILD_COMMIT" \
		--env BUILD_TIME="$$BUILD_TIME" \
		--mount type=volume,source="optimizer_cache",target=/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		cosmwasm/optimizer:0.16.0

.PHONY: fuzz
fuzz:
	cargo test -p tests-fuzz -- --test-threads=1 --nocapture

.PHONY: fuzz-math
fuzz-math:
	cargo test -p tests-fuzz -- math:: --nocapture

.PHONY: fuzz-endurance
fuzz-endurance:
	cargo test -p tests-fuzz -- endurance --ignored --test-threads=1 --nocapture

.PHONY: proto
proto:
	@echo "Generating JSON schemas..."
	@for d in contracts/*/*/; do \
		if [ -f "$$d/src/bin/schema.rs" ]; then \
			echo "  $$d"; \
			(cd "$$d" && cargo run --bin schema 2>/dev/null); \
		fi; \
	done
	@echo "Converting to proto..."
	@cargo run -p schema-to-proto
