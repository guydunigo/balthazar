extern crate balthurner;
extern crate wasmer_runtime;

use balthurner::{wasm, Runner, RunnerResult, WasmRunner};
use std::{env, fs};

fn main() -> RunnerResult<(), wasm::Error> {
    let wasm = {
        let file_name = env::args().nth(1).expect("No wasm file provided.");

        fs::read(file_name).expect("Could not read file")
    };

    let args = env::args().nth(2).expect("No arguments provided.");

    let result = WasmRunner::run(&wasm[..], args.as_bytes())?;

    println!("{:?} gives {:?}", args, result);

    Ok(())
}
