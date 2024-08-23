.PHONY: compile
compile:
	docker run --rm -v .:/code \
		--mount type=volume,source="optimizer_cache",target=/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		cosmwasm/optimizer:0.16.0