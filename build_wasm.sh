#!/bin/sh

cargo build -p balthwasm --target wasm32-unknown-unknown

wasm-gc main.wasm
