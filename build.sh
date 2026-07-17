#!/bin/bash
mkdir -p ./artifacts

if [[ $(uname -m) == "arm64" ]]; then
  OPTIMIZER_IMAGE="cosmwasm/optimizer-arm64:0.17.0"
else
  OPTIMIZER_IMAGE="cosmwasm/optimizer:0.17.0"
fi

docker build --build-arg OPTIMIZER_IMAGE="$OPTIMIZER_IMAGE" -t cosmwasm-optimizer-clang .

# The optimizer container mounts only this dir as /code; the repo .git is at the
# repo root and is NOT visible inside it, so euclid's build.rs cannot read git.
# Capture commit + commit-time on the host here and pass them in as env vars.
eval "$(sh ../../scripts/build-info.sh)"
echo "Build provenance: commit=$BUILD_COMMIT time=$BUILD_TIME"

echo "Building contracts for $(uname -m) architecture"
docker run --rm -v "$(pwd)":/code \
  --env RUSTFLAGS="-C link-arg=-s target-feature=-bulk-memory" \
  --env BUILD_COMMIT="$BUILD_COMMIT" \
  --env BUILD_TIME="$BUILD_TIME" \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm-optimizer-clang