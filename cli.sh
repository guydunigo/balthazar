#!/bin/sh

cargo run --release -- --addr $(cat ./balthachain/chain/parity_account) --jobs-address $(cat ./balthachain/contracts/jobs_address) --jobs-abi ./balthachain/contracts/Jobs.json $*
