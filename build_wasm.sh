#!/bin/sh

cargo build --release -p balthwasm --target wasm32-unknown-unknown

wasm-gc main.wasm
