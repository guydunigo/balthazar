use misc::{
    job::{JobId, TaskId},
    multiaddr, multihash,
};
use proto::{manager::TaskDefiniteErrorKind, DecodeError, EncodeError};
use std::fmt;
use web3::{
    contract::Error as ContractError,
    types::{Address, Log, U256},
};

// TODO: inconsistent naming
// TODO: check every one, remove duplicates, doc, ...
#[derive(Debug)]
pub enum Error {
    MissingLocalAddress,
    MissingJobsContractData,
    Web3(web3::Error),
    Contract(ContractError),
    EthAbi(ethabi::Error),
    // UnsupportedProgramKind(ProgramKind),
    CouldntParseJobsEventName(String),
    CouldntParseJobsEventFromLog(Box<Log>),
    JobsEventDataWrongSize {
        expected: usize,
        got: usize,
        data: Vec<u8>,
    },
    /// When storing a job, an JobNew event is sent with the new nonce for the pending job.
    /// This error is sent when the event couldn't be found.
    CouldntFindJobNewEvent,
    MultiaddrParse(multiaddr::Error),
    Multihash(multihash::DecodeOwnedError),
    TaskStateParse(u64),
    TaskErrorKindParse(u64),
    NotEnoughMoneyInAccount(Address, U256),
    NotEnoughMoneyInPending,
    OtherDataEncodeError(EncodeError),
    OtherDataDecodeError(DecodeError),
    OtherDataDecodeEnumError,
    CouldntDecodeMultihash(misc::multiformats::Error),
    /// There is no job correpsonding to this [`JobId`].
    JobNotFound(JobId),
    /// There is no task correpsonding to this [`TaskId`].
    TaskNotFound(TaskId),
    /// Job is not a draft and therefore can't be modified.
    JobNotADraft(JobId),
    /// Job doesn't have correct values to be set as ready.
    JobNotReady(JobId),
    /// The local address isn't the job's sender.
    JobNotOurs(JobId),
    /// Local address isn't the oracle, so it can't perform the operation.
    LocalAddressNotOracle(Address, Address),
    /// [`TaskDefiniteErrorKind`] is not compatible with the Jobs smart-contract.
    TaskErrorKindNotCompatibleWithJobs(TaskDefiniteErrorKind),
}

impl From<web3::Error> for Error {
    fn from(e: web3::Error) -> Self {
        Error::Web3(e)
    }
}

impl From<ContractError> for Error {
    fn from(e: ContractError) -> Self {
        Error::Contract(e)
    }
}

impl From<ethabi::Error> for Error {
    fn from(e: ethabi::Error) -> Self {
        Error::EthAbi(e)
    }
}

impl From<multiaddr::Error> for Error {
    fn from(e: multiaddr::Error) -> Self {
        Error::MultiaddrParse(e)
    }
}

impl From<misc::multiformats::Error> for Error {
    fn from(e: misc::multiformats::Error) -> Self {
        Error::CouldntDecodeMultihash(e)
    }
}

impl From<multihash::DecodeOwnedError> for Error {
    fn from(e: multihash::DecodeOwnedError) -> Self {
        Error::Multihash(e)
    }
}

impl From<DecodeError> for Error {
    fn from(e: DecodeError) -> Self {
        Error::OtherDataDecodeError(e)
    }
}

impl From<EncodeError> for Error {
    fn from(e: EncodeError) -> Self {
        Error::OtherDataEncodeError(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
