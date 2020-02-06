#[macro_use]
extern crate wasmer_runtime;
extern crate balthamisc as misc;
extern crate bytes;
extern crate either;

use bytes::Bytes;
use either::Either;
use misc::{spawn_thread_async, SpawnThreadError};
use std::{thread::sleep, time::Duration};
use wasmer_runtime::{imports, instantiate, Func, Instance};

pub use wasmer_runtime::error;

// TODO: versions from host api and wasm api

fn sleep_secs(duration: u64) {
    sleep(Duration::from_secs(duration));
}

/// Contains the instance of a parsed WASM program.
///
/// TODO: performance of recreating Instance each run ? Maybe keep compiled form or something ?
pub struct Runner {
    // instance: Instance,
    wasm: Bytes,
}

impl Runner {
    /// Creates a new webassembly runner based on the given wasm program.
    pub fn new(wasm: Bytes) -> Self {
        Runner { wasm }
    }

    pub fn get_instance(&self) -> error::Result<Instance> {
        let import_objects = imports! {
            "env" => {
                "double_host" => func!(|v: i32| v*2),
                "sleep_secs" => func!(sleep_secs),
            },
        };

        let instance = instantiate(&self.wasm[..], &import_objects)?;
        Ok(instance)
    }

    /// Runs the `run` function inside the wasm.
    ///
    /// TODO: performance of recreating the function each time ?
    pub fn run(&self, val: i32) -> error::Result<i32> {
        let instance = self.get_instance()?;
        let run = get_run_fn(&instance)?;

        Ok(run.call(val)?)
    }

    /// Runs on another thread asynchronously.
    pub async fn run_async(&self, val: i32) -> Result<i32, Either<error::Error, SpawnThreadError>> {
        let instance = self.get_instance().map_err(Either::Left)?;

        let result = spawn_thread_async(move || -> error::Result<i32> {
            let run = get_run_fn(&instance)?;
            run.call(val).map_err(|e| e.into())
        })
        .await;

        match result {
            Ok(Ok(val)) => Ok(val),
            Ok(Err(e)) => Err(Either::Left(e)),

            Err(e) => Err(Either::Right(e)),
        }
    }

    /*
    pub fn instance(&self) -> &Instance {
        &self.instance
    }
    */
}

fn get_run_fn<'a>(instance: &'a Instance) -> error::Result<Func<'a, i32, i32>> {
    instance.func("run").map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    extern crate futures;

    use super::*;
    use futures::executor::block_on;
    use std::fs;

    const TEST_FILE: &str = "test_files/test.wasm";

    #[test]
    fn it_executes_correctly_test_file() -> error::Result<()> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let runner = Runner::new(wasm.into());

        assert_eq!(runner.run(3)?, 6);

        Ok(())
    }

    #[test]
    fn it_executes_correctly_test_file_asynchronously(
    ) -> Result<(), Either<error::Error, SpawnThreadError>> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let runner = Runner::new(wasm.into());

        let res = block_on(runner.run_async(3))?;

        assert_eq!(res, 6);

        Ok(())
    }
}
