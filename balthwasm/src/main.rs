extern crate balthwasm as wasm;

use std::time::Instant;

fn main() {
    let arguments = std::env::args().nth(1).expect("you must pass an argument");
    let start = Instant::now();
    let result = wasm::my_run(arguments.into_bytes()).unwrap();
    let end = Instant::now();

    println!("Time: {}ms", (end - start).as_millis());
    println!("Result: {}", String::from_utf8_lossy(&result[..]));
}
