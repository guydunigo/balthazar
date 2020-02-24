extern crate balthurner;
extern crate wasmer_runtime;

use balthurner::{wasm, Runner, RunnerResult, WasmRunner};
use std::time::Instant;
use std::{env, fs};

fn main() -> RunnerResult<(), wasm::Error> {
    let inst_start = Instant::now();
    let wasm = {
        let file_name = env::args().nth(1).expect("No wasm file provided.");

        fs::read(file_name).expect("Could not read file")
    };
    let inst_read = Instant::now();

    let args = env::args().nth(2).expect("No arguments provided.");

    let result = WasmRunner::run(&wasm[..], args.as_bytes())?;
    let inst_res = Instant::now();

    println!(
        "{:?} gives {:?}",
        args,
        String::from_utf8_lossy(&result[..])
    );
    println!(
        "times:\n- read file {}ms\n- running {}ms",
        (inst_read - inst_start).as_millis(),
        (inst_res - inst_read).as_millis(),
    );

    Ok(())
}
