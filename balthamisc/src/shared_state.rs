// TODO: Display
extern crate libp2p;
use super::job::{Address, TaskId};
pub use libp2p::PeerId;
use proto::worker::TaskErrorKind;
use std::{collections::HashMap, fmt, time::SystemTime};

/// State shared between managers used for consensus to track the precise state of
/// every tasks from creation to completion.
#[derive(Clone, Debug, Default)]
pub struct SharedState {
    pub tasks: HashMap<TaskId, Task>,
    // TODO: store/cache jobs ?
    // TODO: store messages ?
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
    completeness: TaskCompleteness,
}

impl Task {
    pub fn new(task_id: TaskId) -> Self {
        Task {
            task_id,
            nb_failures: 0,
            managers_addresses: Vec::new(),
            completeness: TaskCompleteness::default(),
        }
    }

    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    pub fn nb_failures(&self) -> u64 {
        self.nb_failures
    }
    // TODO: prevent modifs if no more incomplete?
    pub fn set_nb_failures(&mut self, new: u64) {
        // TODO: check limit and new value ?
        self.nb_failures = new;
    }

    pub fn managers_addresses(&self) -> &[Address] {
        &self.managers_addresses[..]
    }
    // TODO: prevent modifs if no more incomplete?
    pub fn managers_addresses_mut(&mut self) -> &mut Vec<Address> {
        &mut self.managers_addresses
    }

    pub fn completeness(&self) -> &TaskCompleteness {
        &self.completeness
    }
    pub fn is_incomplete(&self) -> bool {
        self.completeness.is_incomplete()
    }
    pub fn get_substates(&mut self) -> Option<&mut Vec<SubTasksState>> {
        self.completeness.get_substates()
    }

    // TODO: error and no panic!
    pub fn set_definitely_failed(&mut self, reason: TaskErrorKind) {
        if self.is_incomplete() {
            self.completeness = TaskCompleteness::DefinetelyFailed(reason)
        } else {
            panic!("Already no more incomplete!");
        }
    }

    // TODO: error and no panic!
    /// Beware to check that the task is assigned to enough workers and all...
    pub fn set_completed(&mut self, result: Vec<u8>) {
        if let Some(substates) = self.get_substates() {
            self.completeness = TaskCompleteness::Completed {
                result,
                workers_payment_info: substates
                    .drain(..)
                    .filter_map(|s| s.map(Assigned::into_payment_info))
                    .collect(),
            };
        } else {
            panic!("Already no more incomplete!");
        }
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
    /// The task is definetely failed and won't be scheduled again.
    /// The reason is provided to explain.
    DefinetelyFailed(TaskErrorKind),
    /// The task was successfuly computed and here is the selected result.
    /// All involved parties are paid and the sender has been refunded from the
    /// remaining money for this task.
    Completed {
        result: Vec<u8>,
        workers_payment_info: Vec<WorkerPaymentInfo>,
    },
}

impl TaskCompleteness {
    pub fn is_incomplete(&self) -> bool {
        if let TaskCompleteness::Incomplete { .. } = self {
            true
        } else {
            false
        }
    }

    pub fn get_substates(&mut self) -> Option<&mut Vec<SubTasksState>> {
        if let TaskCompleteness::Incomplete { substates } = self {
            Some(substates)
        } else {
            None
        }
    }
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

impl Default for TaskCompleteness {
    fn default() -> Self {
        TaskCompleteness::Incomplete {
            substates: Vec::new(),
        }
    }
}

/// State of a single task replica.
/// If [`None`], then it's pending, otherwise it is assigned to a worker.
pub type SubTasksState = Option<Assigned>;

/// Task replica assigned to a worker to be computed and waiting for result.
#[derive(Clone, Debug)]
pub struct Assigned {
    // TODO: SystemTime or Instant ?
    last_check_timestamp: SystemTime,
    worker: PeerId,
    workers_manager: PeerId,
    payment_info: WorkerPaymentInfo,
}

impl Assigned {
    pub fn new(
        worker: PeerId,
        workers_manager: PeerId,
        worker_address: Address,
        worker_price: u64,
        network_price: u64,
    ) -> Self {
        Assigned {
            last_check_timestamp: SystemTime::now(),
            worker,
            workers_manager,
            payment_info: WorkerPaymentInfo::new(worker_address, worker_price, network_price),
        }
    }

    pub fn last_check_timestamp(&self) -> SystemTime {
        self.last_check_timestamp
    }
    pub fn update_last_check_timestamp(&mut self) {
        self.last_check_timestamp = SystemTime::now();
    }
    pub fn worker(&self) -> &PeerId {
        &self.worker
    }
    pub fn workers_manager(&self) -> &PeerId {
        &self.workers_manager
    }
    pub fn payment_info(&self) -> &WorkerPaymentInfo {
        &self.payment_info
    }
    pub fn into_payment_info(self) -> WorkerPaymentInfo {
        self.payment_info
    }
}

impl fmt::Display for Assigned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Information needed to pay a worker.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerPaymentInfo {
    worker_address: Address,
    worker_price: u64,
    network_price: u64,
}

impl WorkerPaymentInfo {
    pub fn new(worker_address: Address, worker_price: u64, network_price: u64) -> Self {
        WorkerPaymentInfo {
            worker_address,
            worker_price,
            network_price,
        }
    }

    pub fn worker_address(&self) -> &Address {
        &self.worker_address
    }

    pub fn worker_price(&self) -> u64 {
        self.worker_price
    }

    pub fn network_price(&self) -> u64 {
        self.network_price
    }
}

impl fmt::Display for WorkerPaymentInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
