> This project is being realized in the context of my master thesis in the
> **Technische Hochschule Ulm** (Deutschland), based on an idea I had a few years back.

# Balthazar: A peer-to-peer world computer.

## Balthacli

The command-line tool to run the node and interact with the network.
It serves as an interface to **Balthalib** for the command-line and contains only code for this purpose.

## Balthalib

Contains and regroups all the base elements to use the network found in the libraries given here underneath.

## Balthastore

This part manages the different storages used by the system.
For now, only **IPFS** will be implemented but others might come in the future.

## Balthaproto

The different protocols used specifically for this system for inter-node communications.

## Balthurner

The different elements correpsonding to the execution (**runner**) environment can be found it this crate.

## Balthamisc

Contains different miscelaneous types and functions used by other libraries.

---

## Running instructions

### Building
1. Install the `rustup` and `cargo` (See [the official Rust website](https://rust-lang.org))
2. Run `cargo build --release`, it **will** take a long while, continue the next steps in the meantime.

3. In parallel, install the wasm toolchain: `rustup target add wasm32-unkown-unkown`
4. Run `./build_wasm.sh` to compile the example web-assembly program

### Running

> **Note:** This information is outdated and won't work anymore. Sorry.

1. Set up an IPFS daemon locally on default port (See [the official website](https://ipfs.io))
2. Run the manager in one terminal `cargo run --release`
3. Run a worker in another terminal `cargo run --release 1`, it should automatically detect the manager, but if it doesn't, add one of the manager's addresses, for example `cargo run --release 1 /ip4/127.0.0.1/tcp/3333`
