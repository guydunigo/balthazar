use std::{thread::sleep, time::Duration};
use wasmer_runtime::{imports, instantiate, Func, Instance};

use super::{Runner, RunnerResult};
pub use wasmer_runtime::error;

pub use error::Error;

// TODO: versions from host api and wasm api
// TODO: performance of recreating Instance each run ? Maybe keep compiled form or something ?

fn sleep_secs(duration: u64) {
    sleep(Duration::from_secs(duration));
}

/// Uses Wasmer to run the program.
pub struct WasmRunner;

impl WasmRunner {
    fn get_instance(wasm: &[u8]) -> error::Result<Instance> {
        let import_objects = imports! {
            "env" => {
                "double_host" => func!(|v: i32| v*2),
                "sleep_secs" => func!(sleep_secs),
            },
        };

        let instance = instantiate(&wasm[..], &import_objects)?;
        Ok(instance)
    }

    fn get_run_fn<'a>(instance: &'a Instance) -> error::Result<Func<'a, u8, u8>> {
        instance.func("run").map_err(|e| e.into())
    }

    fn get_test_fn<'a>(instance: &'a Instance) -> error::Result<Func<'a, (u8, u8), u8>> {
        instance.func("test").map_err(|e| e.into())
    }
}

impl Runner for WasmRunner {
    type Error = error::Error;

    fn run(program: &[u8], arguments: &[u8]) -> RunnerResult<Vec<u8>, Self::Error> {
        let instance = Self::get_instance(program)?;
        let run = Self::get_run_fn(&instance)?;

        // TODO: actually pass the array
        let result: error::Result<u8> = run.call(arguments[0]).map_err(|e| e.into());
        Ok(vec![result?])
    }

    fn test(program: &[u8], arguments: &[u8], result: &[u8]) -> RunnerResult<bool, Self::Error> {
        let instance = Self::get_instance(program)?;
        let test = Self::get_test_fn(&instance)?;

        // TODO: actually pass the array
        let result: error::Result<u8> = test.call(arguments[0], result[0]).map_err(|e| e.into());
        Ok(result? != 0)
    }
}

#[cfg(test)]
mod tests {
    extern crate futures;

    use super::*;
    use futures::executor::block_on;
    use std::fs;

    const TEST_FILE: &str = "test_files/test.wasm";

    #[test]
    fn it_executes_correctly_test_file() -> RunnerResult<(), error::Error> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let args = vec![3];

        let result = WasmRunner::run(&wasm[..], &args[..])?;

        assert_eq!(result[0], 6);

        Ok(())
    }

    #[test]
    fn it_executes_correctly_test_file_asynchronously() -> RunnerResult<(), error::Error> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let args = vec![3];

        let future = WasmRunner::run_async(&wasm[..], &args[..]);

        let res = block_on(future)?;

        assert_eq!(res[0], 6);

        Ok(())
    }
}
