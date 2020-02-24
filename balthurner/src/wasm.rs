use std::{
    fmt,
    sync::{Arc, RwLock},
};
use wasmer_runtime::{imports, instantiate, Array, Ctx, Func, Instance, WasmPtr};

use super::{Runner, RunnerError, RunnerResult};
pub use wasmer_runtime::error;

#[derive(Debug)]
pub enum Error {
    /// Error with the Wasmer executor (parsing wasm file, executor crash, ...).
    WasmerError(error::Error),
    /// The program returned a negative code, indicating an execution error.
    RuntimeError(host_abi::WasmResult),
    /// The *mutex* data passed to the machine is poisonned (see [`RwLock`]).
    PoisonError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

impl<E: Into<error::Error>> From<E> for Error {
    fn from(e: E) -> Self {
        Error::WasmerError(e.into())
    }
}

impl<E: Into<error::Error>> From<E> for RunnerError<Error> {
    fn from(err: E) -> Self {
        let err: Error = err.into().into();
        RunnerError::InternalError(err)
    }
}

// TODO: versions from host api and wasm api
// TODO: performance of recreating Instance each run ? Maybe keep compiled form or something ?

// TODO: check that no function can crash the whole program...
mod host_abi {
    use std::{
        sync::{Arc, RwLock},
        thread::sleep,
        time::Duration,
    };
    use wasmer_runtime::{Array, Ctx, WasmPtr};

    // TODO: better errors
    pub type WasmResult = i64;
    // const RESULT_OK: WasmResult = 0;
    const RESULT_ERROR: WasmResult = -1;

    pub fn wasm_to_result(res: WasmResult) -> Result<WasmResult, WasmResult> {
        if res < 0 {
            Err(res)
        } else {
            Ok(res)
        }
    }

    /// For now, if the value returned by the test function is:
    /// - `< 0` : an error occured
    /// - `= 0` : incorrect result
    /// - `> 0` : correct result
    pub fn is_test_correct(result: WasmResult) -> bool {
        result > 0
    }

    pub fn sleep_secs(duration: u64) {
        sleep(Duration::from_secs(duration));
    }

    pub fn mark(val: i64) {
        eprintln!("Logging mark from wasm: {}.", val);
    }

    // TODO: get in multiple parts if buffer not big enough ?
    pub fn get_arguments(
        ctx: &mut Ctx,
        args: &[u8],
        ptr: WasmPtr<u8, Array>,
        len: u32,
    ) -> WasmResult {
        if args.len() <= len as usize {
            let memory = ctx.memory(0);

            if let Some(memory_writer) = ptr.deref(memory, 0, len) {
                for (i, b) in args.iter().enumerate() {
                    memory_writer[i].set(*b);
                }

                return args.len() as WasmResult;
            }
        }

        RESULT_ERROR
    }

    // TODO: Set limit ?
    pub fn send_result(
        ctx: &mut Ctx,
        result: Arc<RwLock<Vec<u8>>>,
        ptr: WasmPtr<u8, Array>,
        len: u32,
    ) -> WasmResult {
        if let Ok(mut result) = result.write() {
            result.clear();

            let memory = ctx.memory(0);

            if let Some(memory_writer) = ptr.deref(memory, 0, len) {
                result.resize(len as usize, 0);

                for i in 0..len as usize {
                    result[i] = memory_writer[i].get();
                }

                return len as WasmResult;
            }
        }

        RESULT_ERROR
    }
}

/// Uses Wasmer to run a webassembly program.
pub struct WasmRunner {}

impl WasmRunner {
    fn get_instance(
        wasm: &[u8],
        args: &[u8],
        result: Arc<RwLock<Vec<u8>>>,
    ) -> error::Result<Instance> {
        // TODO: avoid cloning arguments...
        let args = Vec::from(args);

        let import_objects = imports! {
            "env" => {
                "double_host" => func!(|v: i32| v*2),
                "sleep_secs" => func!(host_abi::sleep_secs),
                "mark" => func!(host_abi::mark),
                "host_get_arguments" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32|
                    host_abi::get_arguments(ctx, &args[..], ptr, len)),
                "host_send_result" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32|
                    host_abi::send_result(ctx, result.clone(), ptr, len)),
            },
        };

        let instance = instantiate(&wasm[..], &import_objects)?;
        Ok(instance)
    }

    fn get_run_fn(instance: &Instance) -> error::Result<Func<'_, (), host_abi::WasmResult>> {
        instance.func("run").map_err(|e| e.into())
    }

    fn get_test_fn(instance: &Instance) -> error::Result<Func<'_, (), host_abi::WasmResult>> {
        instance.func("test").map_err(|e| e.into())
    }
}

impl Runner for WasmRunner {
    type Error = Error;

    fn run(program: &[u8], arguments: &[u8]) -> RunnerResult<Vec<u8>, Self::Error> {
        let encoded_res = Arc::new(RwLock::new(Vec::new()));
        let instance = Self::get_instance(program, arguments, encoded_res.clone())?;
        let run = Self::get_run_fn(&instance)?;

        match run.call().map(host_abi::wasm_to_result) {
            Ok(Ok(_)) => Ok(encoded_res.read().map_err(|_| Error::PoisonError)?.clone()),
            Ok(Err(e)) => Err(Error::RuntimeError(e).into()),
            Err(e) => Err(e.into()),
        }
    }

    fn test(program: &[u8], arguments: &[u8], result: &[u8]) -> RunnerResult<bool, Self::Error> {
        let result = Arc::new(RwLock::new(Vec::from(result)));

        let instance = Self::get_instance(program, arguments, result)?;
        let test = Self::get_test_fn(&instance)?;

        match test.call().map(host_abi::wasm_to_result) {
            Ok(Ok(r)) => Ok(host_abi::is_test_correct(r)),
            Ok(Err(e)) => Err(Error::RuntimeError(e).into()),
            Err(e) => Err(e.into()),
        }
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
    fn it_executes_correctly_test_file() -> RunnerResult<(), Error> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let args = b"3";

        let result = WasmRunner::run(&wasm[..], args)?;

        assert_eq!(result, b"6");

        Ok(())
    }

    #[test]
    fn it_executes_correctly_test_file_asynchronously() -> RunnerResult<(), Error> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let args = b"3";

        let future = WasmRunner::run_async(&wasm[..], args);

        let res = block_on(future)?;

        assert_eq!(res, b"6");

        Ok(())
    }
}
