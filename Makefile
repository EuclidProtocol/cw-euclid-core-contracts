.PHONY: compile
compile:
	docker run --rm -v .:/code \
		--mount type=volume,source="optimizer_cache",target=/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		cosmwasm/optimizer:0.16.0

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