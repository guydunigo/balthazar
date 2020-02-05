#!/bin/sh

cargo build --release -p balthwasm --target wasm32-unknown-unknown
which wasm-gc > /dev/null || (echo "Installing 'wasm-gc'..."; cargo install wasm-gc)
wasm-gc test.wasm
