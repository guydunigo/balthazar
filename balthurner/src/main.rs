extern crate balthurner;
extern crate wasmer_runtime;

use balthurner::{error, Runner};
use std::{env, fs};
use wasmer_runtime::Func;

fn main() -> error::Result<()> {
    let wasm = {
        let file_name = env::args().nth(1).expect("No wasm file provided");

        fs::read(file_name).expect("Could not read file")
    };

    let runner = Runner::new(&wasm[..])?;
    let instance = runner.instance();

    let get_six: Func<(), i32> = instance.func("get_six")?;
    let double: Func<i32, i32> = instance.func("double")?;
    let double_from_host: Func<i32, i32> = instance.func("double_from_host")?;

    println!("six: {}", get_six.call()?);
    println!("double 7: {}", double.call(7)?);
    println!("double_from_host 7: {}", double_from_host.call(7)?);
    println!("run sleep and 3*2: {}", runner.run(3)?);

    Ok(())
}
