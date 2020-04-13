extern crate balthastore as store;

use futures::future::{BoxFuture, FutureExt, TryFutureExt};
use misc::{spawn_thread_async, SpawnThreadError};
use std::{
    fmt,
    sync::{Arc, RwLock},
};
use store::{FetchStorage, StoragesWrapper};
use wasmer_runtime::{imports, instantiate, Array, Ctx, Func, Instance, WasmPtr};

use super::{Executor, ExecutorError, ExecutorResult, Handle};
pub use wasmer_runtime::error;

#[derive(Debug)]
pub enum Error {
    /// Error with the Wasmer executor (parsing wasm file, executor crash, ...).
    WasmerError(error::Error),
    /// The *mutex* data passed to the machine is poisonned (see [`RwLock`]).
    PoisonError,
    /// Error when spawning the separate thread for the executor, see [`SpawnThreadError`].
    SpawnThreadError(SpawnThreadError),
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

impl<E: Into<error::Error>> From<E> for ExecutorError<Error> {
    fn from(err: E) -> Self {
        let err: Error = err.into().into();
        ExecutorError::ExecutorError(err)
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
    pub type WasmArgs = ();
    pub type WasmResult = i64;
    // const RESULT_OK: WasmResult = 0;
    pub const RESULT_ERROR: WasmResult = -1;

    pub fn wasm_to_result(res: WasmResult) -> Result<WasmResult, WasmResult> {
        if res < 0 {
            Err(res)
        } else {
            Ok(res)
        }
    }

    /*
    /// For now, if the value returned by the test function is:
    /// - `< 0` : an error occured or incorrect results
    /// - `>= 0` and `< results.len()` : correct results
    /// - `> results.len()` : incorrect results
    pub fn is_test_correct(result: WasmResult) -> bool {
        result > 0
    }
    */

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

    pub fn get_result(
        ctx: &mut Ctx,
        results: &[Vec<u8>],
        index: u32,
        ptr: WasmPtr<u8, Array>,
        len: u32,
    ) -> WasmResult {
        if let Some(result) = results.get(index as usize) {
            get_arguments(ctx, &result[..], ptr, len)
        } else {
            RESULT_ERROR
        }
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

enum RunTest {
    Run(Arc<RwLock<Vec<u8>>>),
    Test(Vec<Vec<u8>>),
}

/// Handle to control the task.
#[derive(Debug, Default, Clone, Copy)]
pub struct WasmHandle {}

impl Handle for WasmHandle {
    fn kill(&mut self) -> BoxFuture<()> {
        unimplemented!();
    }
}

/// Executor to run Webassembly programs.
///
/// Uses Wasmer to run a webassembly program.
#[derive(Default, Clone)]
pub struct WasmExecutor {
    enabled: bool,
    storage: StoragesWrapper,
}

impl WasmExecutor {
    /*
    fn new() -> Self {
        WasmExecutor::default()
    }
    */

    fn get_instance(wasm: &[u8], argument: Vec<u8>, run_test: RunTest) -> error::Result<Instance> {
        // TODO: overflow ?
        let argument_len = argument.len() as u32;
        let (results_len, results_lens, results, result) = match run_test {
            RunTest::Run(result) => (host_abi::RESULT_ERROR, None, None, Some(result)),
            // TODO: overflow ?
            RunTest::Test(results) => (
                results.len() as host_abi::WasmResult,
                // TODO: overflow ?
                Some(results.iter().map(|v| v.len() as i64).collect::<Vec<i64>>()),
                Some(results),
                None,
            ),
        };

        let import_objects = imports! {
            "env" => {
                "double_host" => func!(|v: i32| v*2),
                "sleep_secs" => func!(host_abi::sleep_secs),
                "mark" => func!(host_abi::mark),
                "host_get_argument_len" => func!(move || argument_len),
                "host_get_argument" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32|
                    host_abi::get_arguments(ctx, &argument[..], ptr, len)),
                "host_get_results_len" => func!(move || results_len),
                "host_get_result_len" => func!(move |index: u32|
                    if let Some(results_lens) = &results_lens {
                        results_lens.get(index as usize).copied().unwrap_or(host_abi::RESULT_ERROR)
                    } else {
                        host_abi::RESULT_ERROR
                    }),
                "host_get_result" => func!(move |ctx: &mut Ctx, index: u32, ptr: WasmPtr<u8, Array>, len: u32|
                    if let Some(results) = &results {
                        host_abi::get_result(ctx, &results[..], index, ptr, len)
                    } else {
                        host_abi::RESULT_ERROR
                    }),
                "host_send_result" => func!(move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32|
                    if let Some(result) = &result {
                        host_abi::send_result(ctx, result.clone(), ptr, len)
                    } else {
                        host_abi::RESULT_ERROR
                }),
            },
        };

        let instance = instantiate(&wasm[..], &import_objects)?;
        Ok(instance)
    }

    /// Get a reference to the `run` function of the Wasm program.
    fn get_run_fn(
        instance: &Instance,
    ) -> error::Result<Func<'_, host_abi::WasmArgs, host_abi::WasmResult>> {
        instance.func("run").map_err(|e| e.into())
    }

    /// Get a reference to the `test` function of the Wasm program.
    fn get_test_fn(
        instance: &Instance,
    ) -> error::Result<Func<'_, host_abi::WasmArgs, host_abi::WasmResult>> {
        instance.func("test").map_err(|e| e.into())
    }

    fn spawn_wasm_call_async<'a, 'b, F, Output>(
        program: &'a [u8],
        argument: &'a [u8],
        f: F,
    ) -> (
        BoxFuture<'b, ExecutorResult<Output, <Self as Executor>::Error>>,
        <Self as Executor>::Handle,
    )
    where
        F: FnOnce(Vec<u8>, Vec<u8>) -> ExecutorResult<Output, <Self as Executor>::Error>,
        F: Send + 'static,
        Output: Send + 'static,
    {
        // TODO: Find a way to not copy the whole data (pass ref or pointer) ?
        let program = Vec::from(program);
        let argument = Vec::from(argument);

        let result_fut = async move {
            let result = spawn_thread_async(move || f(program, argument)).await;

            match result {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(e)) => Err(e),
                Err(e) => Err(Error::SpawnThreadError(e).into()),
            }
        }
        .boxed();
        (result_fut, WasmHandle::default())
    }
}

#[allow(clippy::type_complexity)]
impl Executor for WasmExecutor {
    type Error = Error;
    type Handle = WasmHandle;

    /// Since the executor is included in this binary, it is always available given it
    /// is enabled.
    fn is_available(&mut self) -> BoxFuture<bool> {
        futures::future::ready(self.enabled).boxed()
    }

    /// Uses [`StoragesWrapper`] class to determine which storage should be used to fetch
    /// the program and download it.
    fn download_program<'a>(
        &'a mut self,
        address: &'a [u8],
        max_size: u64,
    ) -> BoxFuture<'a, Result<Vec<u8>, ()>> {
        // TODO: forward storage parameters
        self.storage
            .fetch(address, max_size)
            // TODO: copy ?
            .map_ok(|bytes| Vec::from(&bytes[..]))
            .map_err(|_| ())
            .boxed()
    }

    /// Spawns a new thread and run the Wasmer runtime on it.
    fn run(
        &mut self,
        program: &[u8],
        argument: &[u8],
        _timeout: u64,
        _max_network_usage: u64,
    ) -> (
        BoxFuture<ExecutorResult<Vec<u8>, Self::Error>>,
        Self::Handle,
    ) {
        Self::spawn_wasm_call_async(program, argument, |program, argument| {
            let encoded_res = Arc::new(RwLock::new(Vec::new()));

            let instance =
                Self::get_instance(&program[..], argument, RunTest::Run(encoded_res.clone()))?;
            let run = Self::get_run_fn(&instance)?;

            match run.call().map(host_abi::wasm_to_result) {
                Ok(Ok(_)) => Ok(encoded_res.read().map_err(|_| Error::PoisonError)?.clone()),
                Ok(Err(e)) => Err(ExecutorError::RuntimeError(e)),
                Err(e) => Err(e.into()),
            }
        })
    }

    fn test(
        &mut self,
        program: &[u8],
        argument: &[u8],
        results: &[Vec<u8>],
        _timeout: u64,
    ) -> (BoxFuture<ExecutorResult<i64, Self::Error>>, Self::Handle) {
        // TODO: avoid copying
        let results = Vec::from(results);
        Self::spawn_wasm_call_async(program, argument, move |program, argument| {
            let instance = Self::get_instance(&program[..], argument, RunTest::Test(results))?;
            let test = Self::get_test_fn(&instance)?;

            match test.call().map(host_abi::wasm_to_result) {
                Ok(Ok(r)) => Ok(r),
                Ok(Err(e)) => Err(ExecutorError::RuntimeError(e)),
                Err(e) => Err(e.into()),
            }
        })
    }

    fn kill_all(&mut self) -> BoxFuture<Result<(), ()>> {
        unimplemented!();
    }
    fn set_cpu_count(_count: u64) {
        unimplemented!();
    }
    fn set_max_memory(_size: u64) {
        unimplemented!();
    }
    fn set_max_network_speed(_speed: u64) {
        unimplemented!();
    }
}

#[cfg(test)]
mod tests {
    extern crate futures;

    use super::*;
    use futures::executor::block_on;
    use std::fs;

    const TEST_FILE: &str = "test_files/test.wasm";
    const TIMEOUT: u64 = 10;
    const MAX_NETWORK_USAGE: u64 = 0;

    #[test]
    fn it_executes_correctly_test_file() -> ExecutorResult<(), Error> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let args = b"3";

        let result =
            WasmExecutor::default().run_sync(&wasm[..], args, TIMEOUT, MAX_NETWORK_USAGE)?;

        assert_eq!(result, b"6");

        Ok(())
    }

    #[test]
    fn it_executes_correctly_test_file_asynchronously() -> ExecutorResult<(), Error> {
        let wasm =
            fs::read(TEST_FILE).expect(&format!("Could not read test file `{}`", TEST_FILE)[..]);
        let args = b"3";

        let mut exec = WasmExecutor::default();
        let (future, _) = exec.run(&wasm[..], args, TIMEOUT, MAX_NETWORK_USAGE);

        let res = block_on(future)?;

        assert_eq!(res, b"6");

        Ok(())
    }
}
