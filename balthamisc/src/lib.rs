extern crate balthaproto as proto;
pub extern crate multiaddr;
extern crate multibase;
pub extern crate multihash;
extern crate tokio;

use futures::future::poll_fn;
use std::{future::Future, pin::Pin, thread};
use tokio::sync::oneshot::{self, error::RecvError};

pub mod job;
pub mod multiformats;
pub mod shared_state;

mod worker_specs;
pub use worker_specs::WorkerSpecs;

/*
#[derive(Debug, Clone)]
pub struct UnknownValue<T>(T);

impl<T: fmt::Display> fmt::Display for UnknownValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown value: {}", self.0)
    }
}

impl<T: fmt::Debug + fmt::Display> std::error::Error for UnknownValue<T> {}
*/

/// Alias for [`tokio::sync::oneshot::error::RecvError`].
pub type SpawnThreadError = RecvError;

/// Spawns a separate thread and returns the result through the Future.
///
/// TODO: wouldn't it be more efficient to use the tokio::spawn ?
pub fn spawn_thread_async<Output, F>(f: F) -> impl Future<Output = Result<Output, SpawnThreadError>>
where
    Output: Send + 'static,
    F: FnOnce() -> Output,
    F: Send + 'static,
{
    let (tx, mut rx) = oneshot::channel();

    thread::spawn(|| {
        let result = f();
        if tx.send(result).is_err() {
            panic!("Thread can't send value back to `SpawnThreadFuture` object: the receiver channel was dropped!");
        }
    });

    // TODO: Good way of doing it ?
    poll_fn(move |cx| Future::poll(Pin::new(&mut rx), cx))
}

#[cfg(test)]
mod tests {
    use super::spawn_thread_async;
    use futures::executor::block_on;

    #[test]
    fn it_can_spawn_a_thread_and_get_result() {
        let fut = spawn_thread_async(|| 3 + 3);

        let result = block_on(fut).unwrap();

        assert_eq!(result, 3 + 3);
    }
}
