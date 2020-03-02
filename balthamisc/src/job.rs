//! Classes for reperenting a job and its subtasks.
use super::multiformats::encode_multibase_multihash_string;
use multiaddr::Multiaddr;
use multihash::Multihash;
use std::fmt;

// TODO: those are temporary aliases.
/// Identifies a unique job on the network.
pub type JobId = Multihash;
/// Identifies a unique task for a given job.
// TODO: should it also contain the job id ?
pub type TaskId = Multihash;

#[derive(Debug, Clone)]
pub struct UnknownValue<T>(T);

impl<T: fmt::Display> fmt::Display for UnknownValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown value: {}", self.0)
    }
}

impl<T: fmt::Debug + fmt::Display> std::error::Error for UnknownValue<T> {}

const BEST_METHOD_COST: u64 = 0;
const BEST_METHOD_PERFORMANCE: u64 = 1;
/// Method to choose which offer is the best to execute a task.
#[derive(Debug, Clone, Copy)]
pub enum BestMethod {
    /// Choose the cheapest peer's offer.
    Cost,
    /// Choose the offer with the most performant worker.
    Performance,
}

impl Into<u64> for BestMethod {
    fn into(self) -> u64 {
        match self {
            BestMethod::Cost => BEST_METHOD_COST,
            BestMethod::Performance => BEST_METHOD_PERFORMANCE,
        }
    }
}

impl std::convert::TryFrom<u64> for BestMethod {
    type Error = UnknownValue<u64>;

    fn try_from(v: u64) -> Result<BestMethod, Self::Error> {
        match v {
            BEST_METHOD_COST => Ok(BestMethod::Cost),
            BEST_METHOD_PERFORMANCE => Ok(BestMethod::Performance),
            _ => Err(UnknownValue(v)),
        }
    }
}

impl fmt::Display for BestMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
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

impl fmt::Display for ProgramKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = match self {
            ProgramKind::Wasm(_) => "Wasm",
        };
        write!(f, "{}", string)
    }
}

/// Parameters required for the worker to execute the task.
#[derive(Debug, Clone)]
pub struct WorkerParameters {
    pub best_method: BestMethod,
    pub max_worker_price: u128,
    pub min_cpu_count: u64,
    pub min_memory: u64,
    pub max_network_usage: u64,
    pub max_network_price: u128,
    pub min_network_speed: u64,
}

impl fmt::Display for WorkerParameters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Best method: {}", self.best_method)?;
        writeln!(f, "Max worker price: {} money/s", self.max_worker_price)?;
        writeln!(f, "Min CPU count: {}", self.min_cpu_count)?;
        writeln!(f, "Min memory: {} kilobytes", self.min_memory)?;
        writeln!(f, "Max network usage: {} kilobits", self.max_network_usage)?;
        writeln!(
            f,
            "Max network price: {} money/kilobits",
            self.max_network_price
        )?;
        writeln!(
            f,
            "Min network speed: {} kilobits/s",
            self.min_network_speed
        )
    }
}

/// Description of a Job.
#[derive(Debug, Clone)]
pub struct Job<PeerAddress> {
    /// `None` if the job hasn't been sent yet or isn't known.
    pub job_id: Option<JobId>,

    pub program_kind: ProgramKind,
    pub addresses: Vec<Multiaddr>,
    pub arguments: Vec<Vec<u8>>,

    pub timeout: u64,
    pub max_failures: u64,
    pub worker_parameters: WorkerParameters,

    pub redundancy: u64,
    pub includes_tests: bool,

    pub sender: PeerAddress,
    pub nonce: u64,
}

impl<PeerAddress: fmt::Display> fmt::Display for Job<PeerAddress> {
    #[allow(irrefutable_let_patterns)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "---------")?;
        write!(f, "Job id: ")?;
        if let Some(job_id) = &self.job_id {
            writeln!(f, "{}", encode_multibase_multihash_string(job_id))?;
        } else {
            writeln!(f, "Unknown")?;
        }
        writeln!(f, "Program kind: {}", self.program_kind)?;
        write!(f, "Program hash: ")?;
        if let ProgramKind::Wasm(ref hash) = self.program_kind {
            writeln!(f, "{}", encode_multibase_multihash_string(&hash))?;
        } else {
            unreachable!();
            // writeln!(f, "Unknown")?;
        }
        writeln!(f, "Addresses: {:?}", self.addresses)?;
        let arguments: Vec<String> = self
            .arguments
            .iter()
            .map(|e| String::from_utf8_lossy(&e[..]).to_string())
            .collect();
        writeln!(f, "Arguments: {:?}", arguments)?;
        writeln!(f)?;
        writeln!(f, "Timeout: {}s", self.timeout)?;
        writeln!(f, "Max failures: {}", self.max_failures)?;
        writeln!(f)?;
        writeln!(f, "{}", self.worker_parameters)?;
        writeln!(f, "Redundancy: {}", self.redundancy)?;
        writeln!(f, "Includes tests: {}", self.includes_tests)?;
        writeln!(f)?;
        writeln!(f, "Sender: {}", self.sender)?;
        writeln!(f, "Nonce: {}", self.nonce)?;
        writeln!(f, "---------")
    }
}

/*
pub struct Task {
    pub task_id: TaskId,
    pub arguments: Vec<u8>,
}
*/
