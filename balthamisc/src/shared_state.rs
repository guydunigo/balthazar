// TODO: Display
extern crate libp2p;
use super::job::{Address, TaskId};
pub use libp2p::PeerId;
use proto::worker::TaskErrorKind;
use std::{collections::HashMap, fmt};

/// State shared between managers used for consensus to track the precise state of
/// every tasks from creation to completion.
#[derive(Clone, Debug, Default)]
pub struct SharedState {
    pub tasks: HashMap<TaskId, Task>,
    // TODO: store/cache jobs ?
}

impl fmt::Display for SharedState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for task in self.tasks.values() {
            // TODO: display workers_payment_info...
            writeln!(f, "{}", task)?
        }
        Ok(())
    }
}

/// Representation of a task with all information relevant for the shared state.
#[derive(Clone, Debug)]
pub struct Task {
    task_id: TaskId,
    nb_failures: u64,
    managers_addresses: Vec<Address>,
}

impl Task {
    pub fn new(task_id: TaskId) -> Self {
        Task {
            task_id,
            nb_failures: 0,
            managers_addresses: Vec::new(),
        }
    }

    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    pub fn nb_failures(&self) -> u64 {
        self.nb_failures
    }

    pub fn inc_nb_failures(&mut self) {
        // TODO: check limit ?
        self.nb_failures += 1;
    }

    pub fn managers_addresses(&self) -> &[Address] {
        &self.managers_addresses[..]
    }
    pub fn managers_addresses_mut(&mut self) -> &mut Vec<Address> {
        &mut self.managers_addresses
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: display workers_payment_info...
        write!(
            f,
            "Task {{ task_id: {}, nb_failures: {}, managers_addresses: [..] }}",
            self.task_id, self.nb_failures
        )
    }
}

/// State a task can be on the blockchain.
#[derive(Debug, Clone)]
pub enum TaskCompleteness {
    /// The task can still be executed and all.
    Incomplete { substates: Vec<SubTasksState> },
    /// The task was successfuly computed and here is the selected result.
    /// All involved parties are paid and the sender has been refunded from the
    /// remaining money for this task.
    Completed {
        result: Vec<u8>,
        workers_payment_info: Vec<WorkerPaymentInfo>,
    },
    /// The task is definetely failed and won't be scheduled again.
    /// The reason is provided to explain.
    DefinetelyFailed(TaskErrorKind),
}

impl fmt::Display for TaskCompleteness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // TODO: display workers_payment_info...
            TaskCompleteness::Completed { result, .. } => write!(
                f,
                "Completed {{ result: {}, workers_payment_info: [..] }}",
                String::from_utf8_lossy(result),
            ),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// State of a single task replica.
/// If [`None`], then it's pending, otherwise it is assigned to a worker.
pub type SubTasksState = Option<Assigned>;

/// Task replica assigned to a worker to be computed and waiting for result.
#[derive(Clone, Debug)]
pub struct Assigned {
    last_check_timestamp: u64,
    worker: PeerId,
    workers_manager: PeerId,
    payment_info: WorkerPaymentInfo,
}

impl fmt::Display for Assigned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Information needed to pay a worker.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerPaymentInfo {
    workers_address: Address,
    worker_price: u64,
    network_price: u64,
}

impl fmt::Display for WorkerPaymentInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
