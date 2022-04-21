//! Classes for reperenting a job and its subtasks.
extern crate ethereum_types;

use super::multiformats::{
    encode_multibase_multihash_string, try_decode_multibase_multihash_string, Error,
};
pub use ethereum_types::Address;
use multihash::{Code, Multihash, MultihashDigest};
pub use proto::{
    smartcontracts::{BestMethod, OtherData},
    worker::ProgramKind,
};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
    str::FromStr,
};

/// Default hashing algorithm used for job and task ids.
/// Currently using **Keccak256** as it is the default in the Ethereum blockchain.
#[derive(Clone, Copy, Debug)]
pub struct DefaultHash;

impl DefaultHash {
    /// Code object of the default hasher used to perform actions on multihash.
    pub const CODE: Code = Code::Keccak256;
    /// Default hash size in bytes.
    // TODO: get size directly from Code
    pub const SIZE: usize = 32;

    /// Compute the multihash using the default hasher.
    pub fn digest(input: &[u8]) -> Multihash {
        Self::CODE.digest(input)
    }
}

// TODO: those are temporary aliases.
/// Identifies a unique job on the network.
pub type JobId = HashId;
/// Identifies a unique task for a given job.
pub type TaskId = HashId;

/// An Identification based on a hash.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HashId {
    inner: Multihash,
}

impl HashId {
    /// Get the inner multihash.
    pub fn hash(&self) -> &Multihash {
        &self.inner
    }

    /// Get the bytes of the multihash including the self-describing part.
    pub fn as_bytes(&self) -> &[u8] {
        self.hash().digest()
    }

    /// From bytes of a multihash.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Multihash::from_bytes(bytes)?.try_into()
    }

    /// Get the bytes of the multihash including the self-describing part.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes()
    }

    /// Takes the digest and make it into a 32 bytes array.
    pub fn to_bytes32(&self) -> [u8; 32] {
        let mut res = [0; DefaultHash::SIZE];
        res.copy_from_slice(&self.hash().digest()[..DefaultHash::SIZE]);
        res
    }

    /// Calculate JobId.
    pub fn job_id(address: &Address, nonce: u128) -> JobId {
        let mut buffer = Vec::with_capacity(address.0.len() + 16);
        buffer.extend_from_slice(&address[..]);
        buffer.extend_from_slice(&nonce.to_be_bytes()[..]);
        DefaultHash::digest(&buffer[..])
            .try_into()
            .expect("We just built it ourselves.")
    }

    /// Calculate TaskId.
    pub fn task_id(job_id: &JobId, i: u128 /*, argument: &[u8]*/) -> TaskId {
        let mut buffer = Vec::with_capacity(DefaultHash::SIZE + 16 /*+ argument.len()*/);
        buffer.extend_from_slice(job_id.as_bytes());
        buffer.extend_from_slice(&i.to_be_bytes()[..]);
        // buffer.extend_from_slice(&argument[..]);
        DefaultHash::digest(&buffer[..])
            .try_into()
            .expect("We just built it ourselves.")
    }
}

impl TryFrom<Multihash> for HashId {
    type Error = Error;

    fn try_from(src: Multihash) -> Result<Self, Self::Error> {
        // TODO: proper error and TryFrom ?
        let src_code = src.code().try_into()?;
        if src_code != DefaultHash::CODE {
            Err(Error::WrongHashAlgorithm {
                expected: DefaultHash::CODE,
                got: src_code,
            })
        } else {
            Ok(TaskId { inner: src })
        }
    }
}

impl TryFrom<&[u8]> for HashId {
    type Error = Error;

    /// Tries to parse a [`DefaultHash`] from a raw array (not multihash encoding).
    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
        if src.len() != DefaultHash::SIZE {
            Err(Error::WrongSourceLength {
                expected: DefaultHash::SIZE,
                got: src.len(),
            })
        } else {
            Self::try_from(Multihash::wrap(DefaultHash::CODE.into(), src)?)
        }
    }
}

impl From<[u8; DefaultHash::SIZE]> for HashId {
    fn from(src: [u8; DefaultHash::SIZE]) -> Self {
        Self::try_from(&src[..]).expect("We should already have the correct size.")
    }
}

impl From<HashId> for Multihash {
    fn from(hash_id: HashId) -> Self {
        hash_id.inner
    }
}

impl FromStr for HashId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        try_decode_multibase_multihash_string(s)?.try_into()
    }
}

impl fmt::Display for HashId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", encode_multibase_multihash_string(self.hash()))
    }
}

pub const MIN_TIMEOUT: u64 = 10; // TODO: maybe too big ?
pub const MIN_CPU_COUNT: u64 = 1;
pub const MIN_REDUNDANCY: u64 = 1;
pub const MIN_WORKER_PRICE: u64 = 1;
pub const MIN_NETWORK_PRICE: u64 = 1;
pub const MIN_CHECKING_INTERVAL: u64 = 15;
pub const MIN_MAN_PRICE: u64 = 1;
pub const DEFAULT_PURITY: bool = false;

/// Description of a Job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    program_kind: ProgramKind,
    program_addresses: Vec<String>,
    program_hash: Multihash,
    arguments: Vec<Vec<u8>>,

    timeout: u64,
    max_worker_price: u64,
    max_network_usage: u64,
    max_network_price: u64,

    min_checking_interval: u64,
    management_price: u64,

    redundancy: u64,
    max_failures: u64,

    best_method: BestMethod,
    min_cpu_count: u64,
    min_memory: u64,
    min_network_speed: u64,

    is_program_pure: bool,

    // TODO: option to avoid the necessity to use BC?
    sender: Address,
    /// `None` if the job hasn't been sent yet or isn't known.
    nonce: Option<u128>,
}

impl fmt::Display for Job {
    #[allow(irrefutable_let_patterns)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "---------")?;
        write!(f, "Job id: ")?;
        if self.nonce.is_some() {
            writeln!(f, "{}", self.job_id().expect("Already checked option."))?;
        } else {
            writeln!(f, "Unknown")?;
        }
        writeln!(f, "Program kind: {:?}", self.program_kind)?;
        writeln!(
            f,
            "Program hash: {}",
            encode_multibase_multihash_string(&self.program_hash)
        )?;
        writeln!(f, "Addresses: [")?;
        for a in self.program_addresses.iter() {
            writeln!(f, "  {}", a)?;
        }
        writeln!(f, "]")?;
        writeln!(f, "Arguments: [")?;
        for (i, a) in self.arguments.iter().enumerate() {
            write!(f, "  ")?;
            if let Some(job_id) = self.job_id() {
                write!(f, "{}", TaskId::task_id(&job_id, i as u128 /*, &a[..]*/))?;
            } else {
                write!(f, "{}", i)?;
            }
            writeln!(f, " : {}", String::from_utf8_lossy(&a[..]))?;
        }
        writeln!(f, "]")?;
        writeln!(f)?;
        writeln!(f, "Timeout: {}s", self.timeout)?;
        writeln!(f, "Max worker price: {} money/s", self.max_worker_price)?;
        writeln!(f, "Max network usage: {} kilobits", self.max_network_usage)?;
        writeln!(
            f,
            "Max network price: {} money/kilobits",
            self.max_network_price
        )?;
        writeln!(f, "Min checking interval: {}s", self.min_checking_interval)?;
        writeln!(f, "Management price: {} money", self.management_price)?;
        writeln!(f)?;
        writeln!(f, "Redundancy: {}", self.redundancy)?;
        writeln!(f, "Max failures: {}", self.max_failures)?;
        writeln!(f)?;
        writeln!(f, "Best method: {:?}", self.best_method)?;
        writeln!(f, "Min CPU count: {}", self.min_cpu_count)?;
        writeln!(f, "Min memory: {} kilobytes", self.min_memory)?;
        writeln!(
            f,
            "Min network speed: {} kilobits/s",
            self.min_network_speed
        )?;
        writeln!(
            f,
            "Is program pure? {}",
            if self.is_program_pure { "Yes" } else { "No" }
        )?;
        writeln!(f)?;
        writeln!(f, "Sender: {}", self.sender)?;
        write!(f, "Nonce: ")?;
        if let Some(nonce) = &self.nonce {
            writeln!(f, "{}", nonce)?;
        } else {
            writeln!(f, "Unknown")?;
        }
        writeln!(f)?;
        writeln!(f, "Max price: {} money", self.calc_max_price())?;
        writeln!(f, "---------")
    }
}

impl Job {
    // TODO: do we really need all those ?
    pub fn new(
        program_kind: ProgramKind,
        program_addresses: Vec<String>,
        program_hash: Multihash,
        arguments: Vec<Vec<u8>>,
        sender: Address,
    ) -> Self {
        Job {
            program_kind,
            program_addresses,
            program_hash,
            arguments,
            timeout: MIN_TIMEOUT,
            max_worker_price: MIN_WORKER_PRICE,
            max_network_usage: 0,
            max_network_price: MIN_NETWORK_PRICE,
            min_checking_interval: MIN_CHECKING_INTERVAL,
            management_price: MIN_MAN_PRICE,
            redundancy: MIN_REDUNDANCY,
            max_failures: 0,
            best_method: BestMethod::default(),
            min_cpu_count: MIN_CPU_COUNT,
            min_memory: 0,
            min_network_speed: 0,
            is_program_pure: DEFAULT_PURITY,
            sender,
            nonce: None,
        }
    }

    pub fn program_kind(&self) -> ProgramKind {
        self.program_kind
    }
    pub fn set_program_kind(&mut self, new: ProgramKind) {
        self.program_kind = new;
    }
    pub fn program_addresses(&self) -> &[String] {
        &self.program_addresses
    }
    // TODO: setter ?
    pub fn program_hash(&self) -> &Multihash {
        &self.program_hash
    }
    pub fn arguments(&self) -> &[Vec<u8>] {
        &self.arguments
    }
    pub fn timeout(&self) -> u64 {
        self.timeout
    }
    pub fn set_timeout(&mut self, new: u64) {
        self.timeout = if new > MIN_TIMEOUT { new } else { MIN_TIMEOUT };
    }
    pub fn max_worker_price(&self) -> u64 {
        self.max_worker_price
    }
    pub fn set_max_worker_price(&mut self, new: u64) {
        self.max_worker_price = if new > MIN_WORKER_PRICE {
            new
        } else {
            MIN_WORKER_PRICE
        };
    }
    pub fn max_network_usage(&self) -> u64 {
        self.max_network_usage
    }
    pub fn set_max_network_usage(&mut self, new: u64) {
        self.max_network_usage = new;
    }
    pub fn max_network_price(&self) -> u64 {
        self.max_network_price
    }
    pub fn set_max_network_price(&mut self, new: u64) {
        self.max_network_price = if new > MIN_NETWORK_PRICE {
            new
        } else {
            MIN_NETWORK_PRICE
        };
    }
    pub fn min_checking_interval(&self) -> u64 {
        self.min_checking_interval
    }
    pub fn set_min_checking_interval(&mut self, new: u64) {
        self.min_checking_interval = if new > MIN_CHECKING_INTERVAL {
            new
        } else {
            MIN_CHECKING_INTERVAL
        };
    }
    pub fn management_price(&self) -> u64 {
        self.management_price
    }
    pub fn set_management_price(&mut self, new: u64) {
        self.management_price = if new > MIN_MAN_PRICE {
            new
        } else {
            MIN_MAN_PRICE
        };
    }
    pub fn redundancy(&self) -> u64 {
        self.redundancy
    }
    pub fn set_redundancy(&mut self, new: u64) {
        self.redundancy = if new > MIN_REDUNDANCY {
            new
        } else {
            MIN_REDUNDANCY
        };
    }
    pub fn max_failures(&self) -> u64 {
        self.max_failures
    }
    pub fn set_max_failures(&mut self, new: u64) {
        self.max_failures = new;
    }
    pub fn best_method(&self) -> BestMethod {
        self.best_method
    }
    pub fn set_best_method(&mut self, new: BestMethod) {
        self.best_method = new;
    }
    pub fn min_cpu_count(&self) -> u64 {
        self.min_cpu_count
    }
    pub fn set_min_cpu_count(&mut self, new: u64) {
        self.min_cpu_count = if new > MIN_CPU_COUNT {
            new
        } else {
            MIN_CPU_COUNT
        };
    }
    pub fn min_memory(&self) -> u64 {
        self.min_memory
    }
    pub fn set_min_memory(&mut self, new: u64) {
        self.min_memory = new;
    }
    pub fn min_network_speed(&self) -> u64 {
        self.min_network_speed
    }
    pub fn set_min_network_speed(&mut self, new: u64) {
        self.min_network_speed = new;
    }
    pub fn is_program_pure(&self) -> bool {
        self.is_program_pure
    }
    pub fn set_is_program_pure(&mut self, new: bool) {
        self.is_program_pure = new;
    }
    // TODO: setter ?
    pub fn sender(&self) -> Address {
        self.sender
    }
    pub fn nonce(&self) -> &Option<u128> {
        &self.nonce
    }
    pub fn set_nonce(&mut self, new: Option<u128>) {
        self.nonce = new;
    }

    // TODO: self-described error?
    /// Is job correct and ready to be locked as pending on the blockchain?
    /// All values must be defined and above their minimum value
    /// and `program_addresses` and `arguments` must be non-empty.
    pub fn is_ready(&self) -> bool {
        !self.program_addresses.is_empty()
            && !self.arguments.is_empty()
            && self.timeout >= MIN_TIMEOUT
            && self.max_worker_price >= MIN_WORKER_PRICE
            && self.max_network_price >= MIN_NETWORK_PRICE
            && self.min_checking_interval >= MIN_CHECKING_INTERVAL
            && self.management_price >= MIN_MAN_PRICE
            && self.redundancy >= MIN_REDUNDANCY
            && self.min_cpu_count >= MIN_CPU_COUNT
    }

    /// Calculate job id of current job if nonce is set.
    pub fn job_id(&self) -> Option<JobId> {
        self.nonce.map(|nonce| JobId::job_id(&self.sender, nonce))
    }

    /// Calculates the maximum amount of money that can be used by the whole job.
    pub fn calc_max_price(&self) -> u64 {
        self.arguments.len() as u64 * self.calc_max_price_per_task()
    }

    /// Calculates the maximum amount of money that can be used for a single task.
    pub fn calc_max_price_per_task(&self) -> u64 {
        // Workers:
        self.redundancy
            * (self.timeout * self.max_worker_price
                + self.max_network_usage * self.max_network_price)
        // Managers:
            + self.management_price
                * (self.redundancy + self.max_failures)
                * (4 + self.timeout / self.min_checking_interval)
    }

    /// This function creates a protobuf message serializing the data which couldn't be
    /// stored directly in the contract.
    pub fn other_data(&self) -> OtherData {
        // TODO: clone or... ?
        OtherData {
            program_kind: self.program_kind().into(),
            program_addresses: Vec::from(self.program_addresses()),
            program_hash: self.program_hash().to_bytes(),
            best_method: self.best_method().into(),
            min_cpu_count: self.min_cpu_count(),
            min_memory: self.min_memory(),
            min_network_speed: self.min_network_speed(),
            is_program_pure: self.is_program_pure(),
        }
    }
}

/*
pub struct Task {
    pub job_id: JobId,
    pub argument_id: u128,
    pub arguments: Vec<u8>,
}
*/

// TODO: proper Errors...
/// Tries to convert an array of bytes to an Ethereum Address.
pub fn try_bytes_to_address(bytes: &[u8]) -> Result<Address, String> {
    if bytes.len() == Address::len_bytes() {
        Ok(Address::from_slice(bytes))
    } else {
        Err(format!(
            "Lengths not matching, expected `{}`, got `{}`.",
            Address::len_bytes(),
            bytes.len()
        ))
    }
}
