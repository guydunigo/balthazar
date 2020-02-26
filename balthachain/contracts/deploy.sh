#!/bin/sh

truffle compile
truffle network --clean
truffle deploy

jq .abi build/contracts/Jobs.json > Jobs.json
truffle networks | grep Jobs | sed "s/^.*0x//" | tee jobs_address
