#!/bin/bash
mkdir -p ./artifacts

if [[ $(uname -m) == "arm64" ]]; then
  OPTIMIZER_IMAGE="cosmwasm/optimizer-arm64:0.17.0"
else
  OPTIMIZER_IMAGE="cosmwasm/optimizer:0.17.0"
fi

docker build --build-arg OPTIMIZER_IMAGE="$OPTIMIZER_IMAGE" -t cosmwasm-optimizer-clang .

echo "Building contracts for $(uname -m) architecture"
docker run --rm -v "$(pwd)":/code \
  --env RUSTFLAGS="-C link-arg=-s target-feature=-bulk-memory" \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm-optimizer-clang > ./logs.txt 2>&1