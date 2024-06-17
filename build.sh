#!/bin/bash

CWD=`pwd`;

echo "$CWD";

build_contract () {
    local CONTRACT_PATH=$1;

    local CONTRACT=`basename $CONTRACT_PATH`;

    cd $CONTRACT_PATH;
    echo "Building contract $CONTRACT..."
    cargo wasm;
    cargo schema;

    cp -r ./schema "$CWD/artifacts/schema/$CONTRACT"
    cd $CWD;

    local BUILD_TARGET=${CONTRACT//-/_}
    
    local IN_FILE="./target/wasm32-unknown-unknown/release/$BUILD_TARGET.wasm"
    local OUT_FILE="./artifacts/wasm/$CONTRACT.wasm"
    wasm-opt -Os $IN_FILE -o $OUT_FILE
    
    # NOT SO IMPORTANT STEPS
    # Log wasm file sizes at the end of build process
    local IN_FILESIZE=$(($(wc -c <"$IN_FILE") +0))
    local OUT_FILESIZE=$(($(wc -c <"$OUT_FILE") +0))
    local LOG="$BUILD_TARGET \t\t: $IN_FILESIZE \t- $OUT_FILESIZE bytes"
    FILE_LOG="$FILE_LOG\n$LOG"
}

build_category () {
     for directory in contracts/*/; do
        if [[ "$(basename $directory)" = "$1" ]]; then
            echo "Building all contracts in category $(basename $directory)..."
            for contract in $directory/*/; do
                build_contract $contract;
            done
            break
        fi
    done
}

# Helper function to build all contracts with build all command
build_all() {
    for directory in contracts/*/; do
        build_category $(basename $directory)
    done
}

is_contract() {
    for directory in contracts/*/; do
        for contract in $directory/*/; do
            if [[ "$(basename $contract)" = "$1" ]]; then
                return 0
            fi
        done
    done
    return 1
}

is_category() {
    for directory in contracts/*/; do
        if [[ "$(basename $directory)" = "$1" ]]; then
            return 0
        fi
    done
    return 1
}

export RUSTFLAGS="-C link-arg=-s"

#Clear current builds
rm -rf ./target
rm -rf ./artifacts
mkdir -p artifacts/wasm
mkdir -p artifacts/schema

for target in "$@"; do
    if [[ "$target" = "all" ]]; then
        build_all
    elif is_contract $target; then
        build_contract $target
    elif is_category $target; then
        build_category $target
    else
        echo "$target is not a valid target"
        exit 1
    fi
    echo -e "$FILE_LOG"
done
