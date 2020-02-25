extern crate balthurner;
extern crate wasmer_runtime;

use balthurner::{wasm, RunnerResult};
use std::{env, fs::read};

fn main() -> RunnerResult<(), wasm::Error> {
    let file_name = env::args().nth(1).expect("No wasm file provided.");
    let args = env::args().nth(2).expect("No arguments provided.");
    let nb_times = env::args()
        .nth(3)
        .unwrap_or_else(|| "1".to_string())
        .parse()
        .expect("Third argument isn't an integer.");

    let wasm = read(file_name).expect("Could not read file");
    balthurner::run(wasm, args.into_bytes(), nb_times)
}
