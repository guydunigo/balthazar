#[macro_use]
extern crate wasmer_runtime;

use std::{thread::sleep, time::Duration};
use wasmer_runtime::{imports, instantiate, Func, Instance};

pub use wasmer_runtime::error;

// TODO: versions from host api and wasm api

fn sleep_secs(duration: u64) {
    sleep(Duration::from_secs(duration));
}

/// Contains the instance of a parsed WASM program.
pub struct Runner {
    instance: Instance,
}

impl Runner {
    /// Creates a new webassembly runner based on the given wasm program.
    pub fn new(wasm: &[u8]) -> error::Result<Self> {
        let import_objects = imports! {
            "env" => {
                "double_host" => func!(|v: i32| v*2),
                "sleep_secs" => func!(sleep_secs),
            },
        };

        let instance = instantiate(wasm, &import_objects)?;
        Ok(Runner { instance })
    }

    /// Runs the `run` function inside the wasm.
    ///
    /// TODO: performance of recreating the function each time ?
    pub fn run(&self, val: i32) -> error::Result<i32> {
        let run: Func<i32, i32> = self.instance.func("run")?;

        Ok(run.call(val)?)
    }

    pub fn instance(&self) -> &Instance {
        &self.instance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const TEST_FILE: &str = "test_files/test.wasm";

    #[test]
    fn it_executes_correctly_test_file() -> error::Result<()> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let runner = Runner::new(&wasm[..])?;

        assert_eq!(runner.run(3)?, 6);

        Ok(())
    }
}
