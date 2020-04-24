// TODO: Display
extern crate libp2p;
use super::job::{Address, Job, JobId, TaskId};
pub use libp2p::PeerId;
use proto::manager::TaskDefiniteErrorKind;
use std::{collections::HashMap, fmt, time::SystemTime};

/// State shared between managers used for consensus to track the precise state of
/// every tasks from creation to completion.
// TODO: delete jobs (and maybe tasks) when completed.
#[derive(Clone, Debug, Default)]
pub struct SharedState {
    pub tasks: HashMap<TaskId, Task>,
    // TODO: maybe not download and store the whole job, but each parameter individually
    // when needed...
    pub jobs: HashMap<JobId, Job>,
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

impl SharedState {
    // TODO: return Iterator?
    pub fn get_nb_unassigned_per_task(&self) -> Vec<(&TaskId, usize)> {
        self.tasks
            .values()
            .filter_map(|t| t.get_nb_unassigned().map(|nb| (t.task_id(), nb)))
            .collect()
    }

    pub fn get_job_from_task_id(&self, task_id: &TaskId) -> Option<&Job> {
        self.tasks
            .get(task_id)
            .map(|t| self.jobs.get(t.job_id()))
            .flatten()
    }
}

/// Representation of a task with all information relevant for the shared state.
#[derive(Clone, Debug)]
pub struct Task {
    task_id: TaskId,
    nb_failures: u64,
    managers_addresses: Vec<Address>,
    completeness: TaskCompleteness,
    job_id: JobId,
    arg_id: u128,
}

impl Task {
    pub fn new(task_id: TaskId, job_id: JobId, arg_id: u128, redundancy: u64) -> Self {
        Task {
            task_id,
            nb_failures: 0,
            managers_addresses: Vec::new(),
            completeness: TaskCompleteness::new(redundancy),
            job_id,
            arg_id,
        }
    }

    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    pub fn arg_id(&self) -> u128 {
        self.arg_id
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
    pub fn get_substates(&self) -> Option<&[SubTasksState]> {
        self.completeness.get_substates()
    }
    pub fn get_substates_mut(&mut self) -> Option<&mut [SubTasksState]> {
        self.completeness.get_substates_mut()
    }
    pub fn get_substate(&self, worker: &PeerId) -> Option<&Assigned> {
        self.completeness.get_substate(worker)
    }
    pub fn get_substate_mut(&mut self, worker: &PeerId) -> Option<&mut Assigned> {
        self.completeness.get_substate_mut(worker)
    }
    pub fn get_nb_unassigned(&self) -> Option<usize> {
        self.completeness.get_nb_unassigned()
    }

    pub fn unassign(&mut self, worker: &PeerId) -> Option<Assigned> {
        self.completeness.unassign(worker)
    }

    pub fn assign(
        &mut self,
        worker: PeerId,
        workers_manager: PeerId,
        payment_info: WorkerPaymentInfo,
    ) -> Result<(), String> {
        self.completeness
            .assign(worker, workers_manager, payment_info)
    }

    // TODO: error and no panic!
    pub fn set_definitely_failed(&mut self, reason: TaskDefiniteErrorKind) {
        if self.is_incomplete() {
            self.completeness = TaskCompleteness::DefinetelyFailed(reason)
        } else {
            panic!("Already no more incomplete!");
        }
    }

    // TODO: error and no panic!
    /// Beware to check that the task is assigned to enough workers and all...
    pub fn set_completed(&mut self, result: Vec<u8>) {
        if let TaskCompleteness::Incomplete { substates } = &mut self.completeness {
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
    DefinetelyFailed(TaskDefiniteErrorKind),
    /// The task was successfuly computed and here is the selected result.
    /// All involved parties are paid and the sender has been refunded from the
    /// remaining money for this task.
    Completed {
        result: Vec<u8>,
        workers_payment_info: Vec<WorkerPaymentInfo>,
    },
}

impl TaskCompleteness {
    fn new(redundancy: u64) -> Self {
        let mut substates = Vec::with_capacity(redundancy as usize);
        substates.resize(redundancy as usize, None);
        TaskCompleteness::Incomplete { substates }
    }

    pub fn is_incomplete(&self) -> bool {
        if let TaskCompleteness::Incomplete { .. } = self {
            true
        } else {
            false
        }
    }

    pub fn get_substates(&self) -> Option<&[SubTasksState]> {
        if let TaskCompleteness::Incomplete { substates } = self {
            Some(substates)
        } else {
            None
        }
    }
    pub fn get_substates_mut(&mut self) -> Option<&mut [SubTasksState]> {
        if let TaskCompleteness::Incomplete { substates } = self {
            Some(substates)
        } else {
            None
        }
    }

    pub fn get_substate(&self, worker: &PeerId) -> Option<&Assigned> {
        if let TaskCompleteness::Incomplete { substates } = self {
            substates
                .iter()
                .find(|s| s.as_ref().filter(|a| *a.worker() == *worker).is_some())
                .map(|a| a.as_ref())
                .flatten()
        } else {
            None
        }
    }
    pub fn get_substate_mut(&mut self, worker: &PeerId) -> Option<&mut Assigned> {
        if let TaskCompleteness::Incomplete { substates } = self {
            substates
                .iter_mut()
                .find(|s| s.as_ref().filter(|a| *a.worker() == *worker).is_some())
                .map(|a| a.as_mut())
                .flatten()
        } else {
            None
        }
    }

    pub fn get_nb_unassigned(&self) -> Option<usize> {
        if let TaskCompleteness::Incomplete { substates } = self {
            Some(substates.iter().filter(|a| a.is_none()).count())
        } else {
            None
        }
    }

    pub fn unassign(&mut self, worker: &PeerId) -> Option<Assigned> {
        if let TaskCompleteness::Incomplete { substates } = self {
            substates
                .iter_mut()
                .find(|s| s.as_ref().filter(|a| *a.worker() == *worker).is_some())
                .map(|a| a.take())
                .flatten()
        } else {
            None
        }
    }

    // TODO: proper error
    pub fn assign(
        &mut self,
        worker: PeerId,
        workers_manager: PeerId,
        payment_info: WorkerPaymentInfo,
    ) -> Result<(), String> {
        // TODO: test already assign to it.
        if let TaskCompleteness::Incomplete { substates } = self {
            if substates
                .iter()
                .filter_map(|a| a.as_ref())
                .all(|a| *a.worker() != worker)
            {
                if let Some(unassigned_slot) = substates.iter_mut().find(|a| a.is_none()) {
                    unassigned_slot.replace(Assigned::new(worker, workers_manager, payment_info));
                    Ok(())
                } else {
                    Err("No more available slots.".to_string())
                }
            } else {
                Err("Worker already is assigned to this task.".to_string())
            }
        } else {
            Err("Task already no more incomplete.".to_string())
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
    pub fn new(worker: PeerId, workers_manager: PeerId, payment_info: WorkerPaymentInfo) -> Self {
        Assigned {
            last_check_timestamp: SystemTime::now(),
            worker,
            workers_manager,
            payment_info,
        }
    }

    pub fn new_with_details(
        worker: PeerId,
        workers_manager: PeerId,
        worker_address: Address,
        worker_price: u64,
        network_price: u64,
    ) -> Self {
        Self::new(
            worker,
            workers_manager,
            WorkerPaymentInfo::new(worker_address, worker_price, network_price),
        )
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
    /// When unassigning, we need only the worker's and workers manager's PeerIds to
    /// notify them to stop.
    pub fn into_unassigned(self) -> (PeerId, PeerId) {
        (self.worker, self.workers_manager)
    }
}

impl fmt::Display for Assigned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Information needed to pay a worker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
