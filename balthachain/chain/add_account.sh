#!/bin/sh

curl --data "{\"method\":\"parity_newAccountFromSecret\",\"params\":[\"0x$(cat ./parity_private)\",\"$(cat ./parity_password)\"],\"id\":1,\"jsonrpc\":\"2.0\"}" -H \"Content-Type: application/json\" -X POST 127.0.0.1:8545
