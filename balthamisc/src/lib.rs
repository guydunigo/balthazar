extern crate balthaproto as proto;
extern crate tokio;

use futures::future::poll_fn;
use std::{future::Future, pin::Pin, thread};
use tokio::sync::oneshot::{self, error::RecvError};

mod node_type;
pub use node_type::{NodeType, NodeTypeContainer};
mod task_status;
pub use task_status::{TaskErrorKind, TaskStatus};

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
