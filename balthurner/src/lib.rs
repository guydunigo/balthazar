#[macro_use]
extern crate wasmer_runtime;
extern crate balthamisc as misc;
extern crate futures;

use futures::future::BoxFuture;
use std::{error::Error, fmt};

use std::time::Instant;

pub mod wasm;
pub use wasm::WasmExecutor;

/// Errors which can be returned by an executor.
#[derive(Debug)]
pub enum ExecutorError<E> {
    /// Timeout has been reached.
    TimedOut,
    /// Error specific to the executor technology.
    ExecutorError(E),
    /// The program returned an error code.
    RuntimeError(i64),
    /// The program ended unexpectedly.
    ProgramCrash,
    /// The program was killed by the outside of the executor.
    Aborted,
}

impl<E: fmt::Display> fmt::Display for ExecutorError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutorError::TimedOut => write!(f, "The program has timed out."),
            ExecutorError::ExecutorError(err) => {
                write!(f, "The executor has encountered a problem: {}.", err)
            }
            ExecutorError::RuntimeError(code) => write!(
                f,
                "The program has encountered a problem and returned the error code: {}.",
                code
            ),
            ExecutorError::ProgramCrash => write!(f, "The program has crashed."),
            ExecutorError::Aborted => write!(f, "The program was aborted."),
        }
    }
}

impl<E: Error> Error for ExecutorError<E> {}

impl<E> From<E> for ExecutorError<E> {
    fn from(e: E) -> Self {
        ExecutorError::ExecutorError(e)
    }
}

/// Result returned by an executor.
pub type ExecutorResult<T, E> = Result<T, ExecutorError<E>>;

// TODO: should a handle implement Drop to kill the task when the handle is dropped ?
/// Object created when executing a task and used to manage it (kill, get result, ...).
pub trait Handle {
    /*
    // TODO: have task id stored in the handle to find them back ?
    // - This would mean taking them as args
    // - This would depends the executor on ids...
    // - Function to get a new handle for the task or something directly in the executor?
    /// Get the task id of the running task.
    fn task_id(&self) -> &TaskId;
    */

    /// Kill the running task, when called the [`result`] future should return
    /// [`ExecutorError::Aborted`].
    // TODO: result if killing failed ?
    // TODO: consume self ?
    // TODO: what happens when called twice ?
    fn kill(&mut self) -> BoxFuture<()>;
}

// TODO: ability to cache Executor and data for performance ?
// TODO: lifetime and memory freeing ?
// TODO: version number
/// Structure to control an executor technology.
#[allow(clippy::type_complexity)]
pub trait Executor {
    // TODO: Boxed Error...
    /// Error type returned if there was a problem running the task.
    type Error: Error + Send + 'static;

    /// [`Handle`] used for tasks.
    type Handle: Handle;

    /// Checks if the executor can be used and is responding.
    /// Useful when it uses an external service the node doesn't directly control
    /// (such as **Docker**).
    fn is_available(&mut self) -> BoxFuture<bool>;

    /// Downloads the program to be passed to the executor.
    /// It can only return an address or identifier if the executor provides a storage
    /// or caching mechanism of its own.
    // TODO: actual error type
    fn download_program<'a>(
        &'a mut self,
        address: &'a [u8],
        max_size: u64,
    ) -> BoxFuture<'a, Result<Vec<u8>, ()>>;

    /// Run the task with the given arguments.
    /// `program` must be the result of [`download_program`].
    ///
    /// See [`ExecutorError`] for the different errors returned.
    fn run(
        &mut self,
        program: &[u8],
        argument: &[u8],
        timeout: u64,
        max_network_usage: u64,
    ) -> (
        BoxFuture<ExecutorResult<Vec<u8>, Self::Error>>,
        Self::Handle,
    );

    /// Run the tests of the program, and return the index of a correct one.
    /// Any other value is considered as an error.
    /// `program` must be the result of [`download_program`].
    ///
    /// See [`ExecutorError`] for the different errors returned.
    fn test(
        &mut self,
        program: &[u8],
        argument: &[u8],
        results: &[Vec<u8>],
        timeout: u64,
    ) -> (BoxFuture<ExecutorResult<i64, Self::Error>>, Self::Handle);

    /// Tries to kill all running tasks, the future resolves when all tasks of the
    /// executor are killed or an error if it has failed to kill at least one.
    // TODO: result if killing failed ?
    // TODO: list of successfully killed or unsuccessfully killed ?
    fn kill_all(&mut self) -> BoxFuture<Result<(), ()>>;

    /// Sets the CPU count that the executor can use.
    ///
    /// > **Note:** This is not guaranteed to affect already running tasks.
    // TODO: return value with actual chosen value or something ?
    fn set_cpu_count(count: u64);

    /// Sets the maximum memory in kilobytes that the executor can use.
    ///
    /// > **Note:** This is not guaranteed to affect already running tasks.
    // TODO: return value with actual chosen value or something ?
    fn set_max_memory(size: u64);

    /// Sets the maximum network speed in kilobits/seconds that the executor can use.
    ///
    /// > **Note:** This is not guaranteed to affect already running tasks.
    // TODO: return value with actual chosen value or something ?
    fn set_max_network_speed(speed: u64);

    /// Executes [`run`] outside an async executor.
    fn run_sync(
        &mut self,
        program: &[u8],
        argument: &[u8],
        timeout: u64,
        max_network_usage: u64,
    ) -> ExecutorResult<Vec<u8>, Self::Error> {
        let (result_fut, _) = self.run(program, argument, timeout, max_network_usage);
        futures::executor::block_on(result_fut)
    }

    /// Executes [`test`] outside an async executor.
    fn test_sync(
        &mut self,
        program: &[u8],
        argument: &[u8],
        results: &[Vec<u8>],
        timeout: u64,
    ) -> ExecutorResult<i64, Self::Error> {
        let (result_fut, _) = self.test(program, argument, results, timeout);
        futures::executor::block_on(result_fut)
    }
}

pub fn run(
    wasm_program: Vec<u8>,
    arg: Vec<u8>,
    nb_times: usize,
) -> ExecutorResult<(), wasm::Error> {
    let inst_read = Instant::now();
    let nb_times = if nb_times == 0 { 1 } else { nb_times };
    let mut exec = WasmExecutor::default();

    let result = exec.run_sync(&wasm_program[..], &arg[..], 10, 0)?;
    // TODO: store all results
    for _ in 0..(nb_times - 1) {
        exec.run_sync(&wasm_program[..], &arg[..], 10, 0)?;
    }
    let inst_res = Instant::now();

    println!(
        "{:?} gives {:?}",
        String::from_utf8_lossy(&arg[..]),
        String::from_utf8_lossy(&result[..])
    );
    println!(
        "times:\n- running all {}ms\n- running average {}ms",
        (inst_res - inst_read).as_millis(),
        (inst_res - inst_read).as_millis() / (nb_times as u128),
    );

    print!("Testing...");
    let test_result = exec.test_sync(&wasm_program[..], &arg[..], &vec![result][..], 10)?;
    println!(" {}", test_result);

    Ok(())
}

#[cfg(test)]
mod tests {}
