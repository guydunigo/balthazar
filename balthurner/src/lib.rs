#[macro_use]
extern crate wasmer_runtime;
extern crate balthamisc as misc;
extern crate futures;

use futures::future::{BoxFuture, FutureExt};
use misc::{spawn_thread_async, SpawnThreadError};
use std::{fmt, fs, path::PathBuf};

use std::time::Instant;

pub mod wasm;
pub use wasm::WasmRunner;

#[derive(Debug)]
pub enum RunnerError<E> {
    /// Timeout has been reached.
    TimedOut,
    /// The command `test` was run but the program doesn't support tests.
    // TODO: relevant ? should we enforce all programs to include tests ?
    NoTests,
    /// Error specific to the executor technology.
    InternalError(E),
    SpawnThreadError(SpawnThreadError),
}

impl<E: fmt::Debug> fmt::Display for RunnerError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<E: fmt::Debug> std::error::Error for RunnerError<E> {}

impl<E> From<E> for RunnerError<E> {
    fn from(e: E) -> Self {
        RunnerError::InternalError(e)
    }
}

pub type RunnerResult<T, E> = Result<T, RunnerError<E>>;

// TODO: ability to cache Runner for performance ?
pub trait Runner: Send + Sync {
    /// Error type returned if there was a problem running the task.
    type Error: std::error::Error + Send + 'static;

    /// Run the task with the given arguments.
    /// `program` can be the actual program or its address.
    ///
    /// If the program doesn't complete before `timeout` seconds, returns `Err(RunnerError::TimedOut)`.
    fn run(program: &[u8], arguments: &[u8]) -> RunnerResult<Vec<u8>, Self::Error>;

    /// Run the tests if they are included in the program.
    /// Checks if the result seem valid given the arguments.
    /// If the program doesn't complete before `timeout` seconds, returns `Err(RunnerError::TimedOut)`.
    /// If the program doesn't support tests, returns `Err(RunnerError::NoTests)`.
    fn test(program: &[u8], arguments: &[u8], result: &[u8]) -> RunnerResult<bool, Self::Error>;

    /*
    /// Abord the execution of a program.
    fn abord(&self);
    */

    /// Run on another thread asynchronously.
    fn run_async<'a>(
        program: &'a [u8],
        arguments: &'a [u8],
    ) -> BoxFuture<'a, RunnerResult<Vec<u8>, Self::Error>> {
        // TODO: Find a way to not copy the whole data (pass ref or pointer) ?
        let program = Vec::from(program);
        let arguments = Vec::from(arguments);

        async move {
            let result = spawn_thread_async(move || -> RunnerResult<Vec<u8>, Self::Error> {
                Self::run(&program[..], &arguments[..])
            })
            .await;

            match result {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(e)) => Err(e),

                Err(e) => Err(RunnerError::SpawnThreadError(e)),
            }
        }
        .boxed()
    }

    /// Test value on another thread asynchronously.
    fn test_async<'a>(
        program: &'a [u8],
        arguments: &'a [u8],
        result: &'a [u8],
    ) -> BoxFuture<'a, RunnerResult<bool, Self::Error>> {
        // TODO: Find a way to not copy the whole data (pass ref or pointer) ?
        let program = Vec::from(program);
        let arguments = Vec::from(arguments);
        let result = Vec::from(result);

        async move {
            let result = spawn_thread_async(move || -> RunnerResult<bool, Self::Error> {
                Self::test(&program[..], &arguments[..], &result[..])
            })
            .await;

            match result {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(e)) => Err(e),

                Err(e) => Err(RunnerError::SpawnThreadError(e)),
            }
        }
        .boxed()
    }
}

pub fn run(
    wasm_file_path: PathBuf,
    args: Vec<u8>,
    nb_times: usize,
) -> RunnerResult<(), wasm::Error> {
    let wasm = fs::read(wasm_file_path).expect("Could not read file");
    let inst_read = Instant::now();
    let nb_times = if nb_times == 0 { 1 } else { nb_times };

    let result = WasmRunner::run(&wasm[..], &args[..])?;
    for _ in 0..(nb_times - 1) {
        WasmRunner::run(&wasm[..], &args[..])?;
    }
    let inst_res = Instant::now();

    println!(
        "{:?} gives {:?}",
        String::from_utf8_lossy(&args[..]),
        String::from_utf8_lossy(&result[..])
    );
    println!(
        "times:\n- running all {}ms\n- running average {}ms",
        (inst_res - inst_read).as_millis(),
        (inst_res - inst_read).as_millis() / (nb_times as u128),
    );

    Ok(())
}

#[cfg(test)]
mod tests {}
