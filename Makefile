# .PHONY: compile
# compile:
# 	docker run --rm -v .:/code \
# 		--mount type=volume,source="optimizer_cache",target=/target \
# 		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
# 		cosmwasm/optimizer:0.16.0

.PHONY: check
check:
	cargo check

.PHONY: clippy
clippy:
	cargo clippy

PHONY: test
test: unit-test

.PHONY: unit-test
unit-test:
	cargo unit-test

# This is a local build with debug-prints activated. Debug prints only show up
# in the local development chain (see the `start-server` command below)
# and mainnet won't accept contracts built with the feature enabled.
.PHONY: build _build
build: _build compress-wasm
_build:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --target wasm32-unknown-unknown --features="debug-print"

# This is a build suitable for uploading to mainnet.
# Calls to `debug_print` get removed by the compiler.
.PHONY: build-mainnet _build-mainnet
build-mainnet: _build-mainnet compress-wasm
_build-mainnet:
	RUSTFLAGS='-C link-arg=-s' cargo build --release --target wasm32-unknown-unknown

# like build-mainnet, but slower and more deterministic
.PHONY: build-mainnet-reproducible
build-mainnet-reproducible:
	docker run --rm -v "$$(pwd)":/contract \
		--mount type=volume,source="$$(basename "$$(pwd)")_cache",target=/code/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		mr7uca/wasm-contract-optimizer:0.0.10

.PHONY: compress-wasm
compress-wasm:
	cp ./target/wasm32-unknown-unknown/release/*.wasm ./contract.wasm
	@## The following line is not necessary, may work only on linux (extra size optimization)
	@# wasm-opt -Os ./contract.wasm -o ./contract.wasm
	cat ./contract.wasm | gzip -9 > ./contract.wasm.gz

.PHONY: schema
schema:
	cargo run --example schema

# Run local development chain with four funded accounts (named a, b, c, and d)
.PHONY: start-server
start-server: # CTRL+C to stop
	docker run -it --rm \
		-p 26657:26657 -p 26656:26656 -p 1317:1317 -p 5000:5000 \
		-v "$$(pwd)":/root/code \
		--name localsecret ghcr.io/scrtlabs/localsecret:v1.6.0

# This relies on running `start-server` in another console
# You can run other commands on the secretcli inside the dev image
# by using `docker exec localsecret secretcli`.
.PHONY: store-contract-local
store-contract-local:
	docker exec localsecret secretcli tx compute store -y --from a  --gas auto --gas-prices 0.0125uscrt --gas-adjustment 1.3 -y -b sync /root/code/optimized-wasm/factory.wasm.gz

.PHONY: integration-test
integration-test:
	npx ts-node tests/integration.ts

.PHONY: clean
clean:
	cargo clean
	-rm -f ./contract.wasm ./contract.wasm.gz