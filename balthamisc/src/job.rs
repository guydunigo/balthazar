//! Classes for reperenting a job and its subtasks.
use multiaddr::Multiaddr;
use multihash::Multihash;
use std::fmt;

// TODO: those are temporary aliases.
/// Identifies a unique job on the network.
pub type JobId = u64;
/// Identifies a unique task for a given job.
// TODO: should it also contain the job id ?
pub type TaskId = u64;

#[derive(Debug, Clone)]
pub struct UnknownValue<T>(T);

impl<T: fmt::Display> fmt::Display for UnknownValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown value: {}", self.0)
    }
}

impl<T: fmt::Debug + fmt::Display> std::error::Error for UnknownValue<T> {}

const BEST_METHOD_PERFORMANCE: u64 = 0;
const BEST_METHOD_COST: u64 = 1;
/// Method to choose which offer is the best to execute a task.
#[derive(Debug, Clone, Copy)]
pub enum BestMethod {
    /// Choose the offer with the most performant worker.
    Performance,
    /// Choose the cheapest peer's offer.
    Cost,
}

impl Into<u64> for BestMethod {
    fn into(self) -> u64 {
        match self {
            BestMethod::Performance => BEST_METHOD_PERFORMANCE,
            BestMethod::Cost => BEST_METHOD_COST,
        }
    }
}

impl std::convert::TryFrom<u64> for BestMethod {
    type Error = UnknownValue<u64>;

    fn try_from(v: u64) -> Result<BestMethod, Self::Error> {
        match v {
            BEST_METHOD_PERFORMANCE => Ok(BestMethod::Performance),
            BEST_METHOD_COST => Ok(BestMethod::Cost),
            _ => Err(UnknownValue(v)),
        }
    }
}

const PROGRAM_KIND_WASM: u64 = 0;
/// Kind of program to execute.
#[derive(Debug, Clone)]
pub enum ProgramKind {
    /// Webassembly program, along with a hash to verify the program.
    Wasm(Multihash),
}

impl Into<(u64, Option<Multihash>)> for ProgramKind {
    fn into(self) -> (u64, Option<Multihash>) {
        match self {
            ProgramKind::Wasm(hash) => (PROGRAM_KIND_WASM, Some(hash)),
        }
    }
}

impl std::convert::TryFrom<(u64, Option<Multihash>)> for ProgramKind {
    type Error = UnknownValue<(u64, Option<Multihash>)>;

    fn try_from(v: (u64, Option<Multihash>)) -> Result<ProgramKind, Self::Error> {
        match v {
            (PROGRAM_KIND_WASM, Some(hash)) => Ok(ProgramKind::Wasm(hash)),
            _ => Err(UnknownValue(v)),
        }
    }
}

/// Parameters required for the worker to execute the task.
#[derive(Debug, Clone)]
pub struct WorkerParameters {
    pub best_method: BestMethod,
    pub max_worker_price: u64,
    pub min_cpu_count: u64,
    pub min_memory: u64,
    pub min_network_speed: u64,
    pub max_network_usage: u64,
}

/// Description of a Job.
#[derive(Debug, Clone)]
pub struct Job<PeerId> {
    pub job_id: JobId,

    pub program_kind: ProgramKind,
    pub addresses: Vec<Multiaddr>,
    pub arguments: Vec<Vec<u8>>,

    pub timeout: u64,
    pub max_failures: u64,
    pub worker_parameters: WorkerParameters,

    pub redundancy: u64,
    pub includes_tests: bool,

    pub sender: PeerId,
}

pub struct Task {
    pub task_id: TaskId,
    pub arguments: Vec<u8>,
}
