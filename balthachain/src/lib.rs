extern crate balthamisc as misc;
extern crate balthaproto as proto;
extern crate ethabi;
extern crate futures;
extern crate web3;

mod config;
pub use config::{Address, ChainConfig};
use ethabi::Event;
use futures::{compat::Compat01As03, executor::block_on, future, Stream, StreamExt};
use misc::{
    job::{BestMethod, Job, JobId, OtherData, ProgramKind, TaskId, UnknownValue, HASH_SIZE},
    multiaddr,
    multihash::{self, Multihash},
};
use proto::{DecodeError, EncodeError, Message};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use web3::{
    contract::{Contract, Error as ContractError, Options},
    transports::{EventLoopHandle, WebSocket},
    types::{self, Block, BlockId, BlockNumber, FilterBuilder, Log},
    Web3,
};

#[derive(Debug)]
pub enum Error {
    MissingLocalAddress,
    MissingJobsContractData,
    Web3(web3::Error),
    Contract(ContractError),
    EthAbi(ethabi::Error),
    UnsupportedProgramKind(ProgramKind),
    CouldntParseJobsEvent(String),
    CouldntParseJobsEventFromLog(Box<Log>),
    JobsEventDataWrongSize {
        expected: usize,
        got: usize,
        data: Vec<u8>,
    },
    /// When storing a job, an JobNew event is sent with the new nonce for the pending job.
    /// This error is sent when the event couldn't be found.
    CouldntFindJobNonceEvent,
    MultiaddrParse(multiaddr::Error),
    Multihash(multihash::DecodeOwnedError),
    TaskStateParse(UnknownValue<u64>),
    TaskErrorKindParse(UnknownValue<u64>),
    NotEnoughMoneyInLocalAccount,
    NotEnoughMoneyInPending,
    OtherDataEncodeError(EncodeError),
    OtherDataDecodeError(DecodeError),
    OtherDataDecodeEnumError,
    CouldntDecodeMultihash(misc::multiformats::Error),
    JobNotComplete, // TODO: ? (Job),
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

#[derive(Clone, Debug)]
pub enum RunMode {
    Block,
    Balance(Option<Address>),
    /*
    JobsCounterGet,
    JobsCounterSet(u128),
    JobsCounterInc,
    */
    /// Subscribe to Jobs smart contract given events or if none are provided, all of them.
    JobsSubscribe(Vec<JobsEventKind>),
    /// List all Jobs smart contract events.
    JobsEvents,
    /// Create draft job and return the cost to pay.
    JobsNewDraft {
        program_kind: ProgramKind,
        addresses: Vec<String>,
        program_hash: Multihash,
        arguments: Vec<Vec<u8>>,
        timeout: u64,
        max_worker_price: u64,
        max_network_usage: u64,
        max_network_price: u64,
        redundancy: u64,
        max_failures: u64,
        min_network_speed: u64,
        best_method: BestMethod,
        min_cpu_count: u64,
        min_memory: u64,
        is_program_pure: bool,
    },
    /// Get an already validated job.
    /// As well as its tasks statuses.
    JobsGetJob {
        job_id: JobId,
    },
    /// Get a pending draft job information.
    JobsGetDraftJob {
        nonce: u128,
    },
    /*
    /// Get a list of pending job ids.
    // TODO
    JobsGetPendingJobs,
    */
    /// Get a list of pending drafts nonces.
    JobsGetDraftJobs,
    /// Remove a draft job.
    JobsDeleteDraftJob {
        nonce: u128,
    },
    /// Query a result if it exists.
    // TODO
    JobsGetResult {
        task_id: TaskId,
    },
    /// Send a result and workers to the sc.
    /// There must be as many workers as job's `redundancy`.
    // TODO
    JobsSetResult {
        task_id: TaskId,
        result: Vec<u8>,
        // Workers ethereum address, worker_price, network_price.
        workers: Vec<(Address, u64, u64)>,
    },
    */
    /// Get money amounts on both locked and pending accounts.
    JobsGetMoney,
    /// Send money to pending account.
    JobsSendMoney {
        amount: u128,
    },
    /// Recover money from pending account.
    JobsRecoverMoney {
        amount: u128,
    },
    /// Set a job as ready, if it meets readiness criteria and there's enough pending
    /// money, will lock the job and send the tasks for execution.
    JobsReady {
        nonce: u128,
    },
}

pub fn run(mode: &RunMode, config: &ChainConfig) -> Result<(), Error> {
    block_on(run_async(mode, config))
}

async fn run_async(mode: &RunMode, config: &ChainConfig) -> Result<(), Error> {
    let chain = Chain::new(config);

    match mode {
        RunMode::Block => {
            let block_id: BlockId = BlockNumber::Latest.into();
            let val = chain.block(block_id.clone()).await?;

            if let Some(val) = val {
                println!("Latest block information for `{:?}`:\n{:?}", block_id, val);
            } else {
                println!("No block found for `{:?}`", block_id);
            }
        }
        RunMode::Balance(Some(addr)) => {
            let val = chain.balance(*addr).await?;
            println!("Balance of `{}` is {}.", addr, val);
        }
        RunMode::Balance(None) => {
            let val = chain.local_balance().await?;
            println!(
                "Balance of local address `{}` is {}.",
                chain.local_address()?,
                val
            );
        }
        RunMode::JobsCounterGet => {
            let val = chain.jobs_counter().await?;
            println!("Value of counter: {}.", val);
        }
        RunMode::JobsCounterSet(new) => {
            chain.jobs_set_counter(*new).await?;
            println!("Counter set to {}.", new);
        }
        RunMode::JobsCounterInc => {
            chain.jobs_inc_counter().await?;
            println!("Counter increased.");
        }
        RunMode::JobsSubscribe(events) => {
            if events.is_empty() {
                let stream = Box::new(chain.jobs_subscribe().await?);
                stream
                    .for_each(|e| {
                        match e {
                            Ok(evt) => println!("{}", evt),
                            Err(_) => println!("{:?}", e),
                        }
                        future::ready(())
                    })
                    .await;
            } else {
                let stream = chain.jobs_subscribe_to_event_kinds(&events[..]).await?;
                stream
                    .for_each(|e| {
                        println!("{:?}", e);
                        future::ready(())
                    })
                    .await;
            }
        }
        RunMode::JobsEvents => {
            for evt in chain.jobs_events()? {
                println!("{:?}, {}", evt, evt.signature());
            }
        }
        RunMode::JobsNewDraft {
            program_kind,
            addresses,
            program_hash,
            arguments,
            timeout,
            max_failures,
            best_method,
            max_worker_price,
            min_cpu_count,
            min_memory,
            max_network_usage,
            max_network_price,
            min_network_speed,
            redundancy,
            is_program_pure,
        } => {
            let mut job = Job::new(
                *program_kind,
                addresses.clone(),
                program_hash.clone(),
                arguments.clone(),
                chain.local_address()?,
            );
            job.set_timeout(*timeout);
            job.set_max_failures(*max_failures);
            job.set_best_method(*best_method);
            job.set_max_worker_price(*max_worker_price);
            job.set_min_cpu_count(*min_cpu_count);
            job.set_min_memory(*min_memory);
            job.set_max_network_usage(*max_network_usage);
            job.set_max_network_price(*max_network_price);
            job.set_min_network_speed(*min_network_speed);
            job.set_redundancy(*redundancy);
            job.set_is_program_pure(*is_program_pure);
            job.set_nonce(None);

            let nonce = chain.jobs_new_draft(&job).await?;

            println!("{}", chain.jobs_get_draft_job(nonce).await?);
            println!("Draft stored !");
            println!("You might still need to send the money for it and set it as ready.");
        }
        RunMode::JobsGetJob { job_id } => {
            let job = chain.jobs_get_job(job_id).await?;
            println!("{}", job);
        }
        RunMode::JobsGetDraftJob { nonce } => {
            let job = chain.jobs_get_draft_job(*nonce).await?;
            println!("{}", job);
        }
        RunMode::JobsGetPendingJobs => {
            let jobs: Vec<String> = chain
                .jobs_get_pending_jobs()
                .await?
                .iter()
                .map(|h| format!("{}", h))
                .collect();
            println!("{:?}", jobs);
        }
        RunMode::JobsGetDraftJobs => {
            let jobs = chain.jobs_get_draft_jobs().await?;
            println!("{:?}", jobs);
        }
        RunMode::JobsDeleteDraftJob { nonce } => {
            let job = chain.jobs_get_draft_job(*nonce).await?;
            println!("{}", job);
            chain.jobs_delete_draft_job(*nonce).await?;
            println!("Deleted!");
        }
        RunMode::JobsGetResult { task_id } => {
            let result = chain.jobs_get_result(task_id).await?;
            println!(
                "Result of task `{}`:\n{}",
                task_id,
                String::from_utf8_lossy(&result[..])
            );
        }
        RunMode::JobsSetResult {
            task_id,
            result,
            workers,
        } => {
            chain.jobs_set_result(task_id, result, &workers[..]).await?;
            println!("Result of task `{}` stored.", task_id,);
        }
        RunMode::JobsGetMoney => {
            let (pending, locked) = chain.jobs_get_pending_locked_money().await?;
            println!(
                "Pending money: {} money.\nLocked money: {} money.",
                pending, locked
            );
        }
        RunMode::JobsSendMoney { amount } => {
            chain.jobs_send_pending_money((*amount).into()).await?;
            println!("{} money sent.", amount);
        }
        RunMode::JobsRecoverMoney { amount } => {
            chain.jobs_recover_pending_money((*amount).into()).await?;
            println!("{} money recovered.", amount);
        }
        RunMode::JobsReady { nonce } => {
            let job = chain.jobs_get_draft_job(*nonce).await?;
            println!("{}", job);
            chain.jobs_ready(*nonce).await?;
            println!("{} set as ready for work.", nonce);
        }
        RunMode::JobsValidate { job_id } => {
            let job = chain.jobs_get_job(job_id).await?;
            println!("{}", job);
            chain.jobs_validate_results(job_id).await?;
            println!("{} validated, workers paid, and money available.", job_id);
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub enum JobsEvent {
    CounterHasNewValue(u128),
    JobNew { sender: Address, nonce: u128 },
    JobPending { job_id: JobId },
    TaskPending { task_id: TaskId },
    NewResult { task_id: TaskId, result: Vec<u8> },
    PendingMoneyChanged { account: Address, new_val: u128 },
}

impl fmt::Display for JobsEvent {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsEvent::JobPending { job_id } => write!(fmt, "JobPending {{ job_id: {} }}", job_id,),
            JobsEvent::TaskPending { task_id } => {
                write!(fmt, "TaskPending {{ task_id: {} }}", task_id,)
            }
            JobsEvent::NewResult { task_id, result } => write!(
                fmt,
                "NewResult {{ task_id: {}, result: {} }}",
                task_id,
                String::from_utf8_lossy(&result[..])
            ),
            _ => write!(fmt, "{:?}", self),
        }
    }
}

impl TryFrom<(&ethabi::Contract, Log)> for JobsEvent {
    type Error = Error;

    // TODO: ugly ?
    /// We assume that [`Log::data`]] is just all the arguments of the events as `[u8; 256]`
    /// concatenated together.
    fn try_from((contract, log): (&ethabi::Contract, Log)) -> Result<Self, Self::Error> {
        for t in log.topics.iter() {
            if let Some(evt) = contract.events().find(|e| e.signature() == *t) {
                match JobsEventKind::try_from(&evt.name[..])? {
                    JobsEventKind::CounterHasNewValue => {
                        let len = log.data.0.len();
                        if len == 32 {
                            let counter =
                                types::U128::from_big_endian(&log.data.0[(len - 16)..len])
                                    .as_u128();
                            return Ok(JobsEvent::CounterHasNewValue(counter));
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected: 32,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::JobNew => {
                        let len = log.data.0.len();
                        let expected = 32 * 2;
                        if len == expected {
                            let sender =
                                Address::from_slice(&log.data.0[(32 - Address::len_bytes())..32]);
                            let nonce =
                                types::U128::from_big_endian(&log.data.0[(64 - 16)..64]).as_u128();
                            return Ok(JobsEvent::JobNew { sender, nonce });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::JobPending => {
                        let len = log.data.0.len();
                        let expected = HASH_SIZE;
                        if len == expected {
                            let job_id = (&log.data.0[..]).try_into()?;
                            return Ok(JobsEvent::JobPending { job_id });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::TaskPending => {
                        let len = log.data.0.len();
                        let expected = HASH_SIZE;
                        if len == expected {
                            let task_id = (&log.data.0[..]).try_into()?;
                            return Ok(JobsEvent::TaskPending { task_id });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::NewResult => {
                        let len = log.data.0.len();
                        let expected = HASH_SIZE + 32;
                        if len > expected {
                            let task_id = (&log.data.0[..HASH_SIZE]).try_into()?;
                            return Ok(JobsEvent::NewResult {
                                task_id,
                                result: Vec::from(&log.data.0[32..]),
                            });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::PendingMoneyChanged => {
                        let len = log.data.0.len();
                        let expected = 32 * 2;
                        if len == expected {
                            let account =
                                Address::from_slice(&log.data.0[(32 - Address::len_bytes())..32]);
                            // TODO: upper_u128 ?
                            let new_val =
                                types::U256::from_big_endian(&log.data.0[32..64]).low_u128();
                            return Ok(JobsEvent::PendingMoneyChanged { account, new_val });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                }
            }
        }

        Err(Error::CouldntParseJobsEventFromLog(Box::new(log)))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JobsEventKind {
    CounterHasNewValue,
    JobNew,
    JobPending,
    TaskPending,
    NewResult,
    PendingMoneyChanged,
}

impl std::convert::TryFrom<&str> for JobsEventKind {
    type Error = Error;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        match src {
            "CounterHasNewValue" => Ok(JobsEventKind::CounterHasNewValue),
            "JobNew" => Ok(JobsEventKind::JobNew),
            "JobPending" => Ok(JobsEventKind::JobPending),
            "TaskPending" => Ok(JobsEventKind::TaskPending),
            "NewResult" => Ok(JobsEventKind::NewResult),
            "PendingMoneyChanged" => Ok(JobsEventKind::PendingMoneyChanged),
            _ => Err(Error::CouldntParseJobsEvent(String::from(src))),
        }
    }
}

impl std::str::FromStr for JobsEventKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl fmt::Display for JobsEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Chain<'a> {
    eloop: EventLoopHandle,
    web3: Web3<WebSocket>,
    config: &'a ChainConfig,
}

impl<'a> Chain<'a> {
    pub fn new(config: &'a ChainConfig) -> Self {
        let (eloop, transport) = WebSocket::new(config.web3_ws()).unwrap();
        Chain {
            eloop,
            web3: web3::Web3::new(transport),
            config,
        }
    }

    pub fn local_address(&self) -> Result<Address, Error> {
        if let Some(addr) = self.config.ethereum_address() {
            Ok(*addr)
        } else {
            Err(Error::MissingLocalAddress)
        }
    }

    pub async fn block(&self, block_id: BlockId) -> Result<Option<Block<types::H256>>, Error> {
        Compat01As03::new(self.web3.eth().block(block_id))
            .await
            .map_err(Error::Web3)
    }

    /// Get balance of given account.
    pub async fn balance(&self, addr: Address) -> Result<types::U256, Error> {
        Compat01As03::new(self.web3.eth().balance(addr, None))
            .await
            .map_err(Error::Web3)
    }

    /// Get balance if provided account if [`ChainConfig::ethereum_address`] is `Some(_)`.
    pub async fn local_balance(&self) -> Result<types::U256, Error> {
        if let Some(addr) = self.config.ethereum_address() {
            self.balance(*addr).await
        } else {
            Err(Error::MissingLocalAddress)
        }
    }

    /// Access the **Jobs** smart-contract at the provided address.
    /// If [`ChainConfig::contract_jobs`] is `None`, returns `None`.
    fn jobs(&self) -> Result<Contract<WebSocket>, Error> {
        if let Some((job_addr, abi)) = self.config.contract_jobs() {
            let c = Contract::from_json(self.web3.eth(), *job_addr, &abi[..])
                .map_err(ContractError::Abi)?;
            Ok(c)
        } else {
            Err(Error::MissingJobsContractData)
        }
    }

    /// Check the [`ethabi::Contract`] object of the **Jobs** smart-contract for advanced
    /// manipulation.
    /// If [`ChainConfig::contract_jobs`] is `None`, returns `None`.
    pub fn jobs_ethabi(&self) -> Result<ethabi::Contract, Error> {
        if let Some((_, abi)) = self.config.contract_jobs() {
            let c = ethabi::Contract::load(&abi[..])?;
            Ok(c)
        } else {
            Err(Error::MissingJobsContractData)
        }
    }

    pub async fn jobs_counter(&self) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("counter", (), addr, Default::default(), None);
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_set_counter(&self, new: u128) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call_with_confirmations("set_counter", new, addr, Default::default(), 0);
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_inc_counter(&self) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call_with_confirmations("inc_counter", (), addr, Default::default(), 0);
        Ok(Compat01As03::new(fut).await?)
    }

    /// Subscribe to all events on given contract.
    pub async fn jobs_subscribe(
        &self,
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        let jobs = self.jobs()?;
        let filter = FilterBuilder::default()
            .address(vec![jobs.address()])
            .build();

        let stream_fut = Compat01As03::new(self.web3.eth_subscribe().subscribe_logs(filter));
        let stream = Compat01As03::new(stream_fut.await?);

        let jobs_ethabi = self.jobs_ethabi()?;
        Ok(stream.map(move |e| match e {
            Ok(log) => (&jobs_ethabi, log).try_into(),
            Err(e) => Err(Error::Web3(e)),
        }))
    }

    /// Subscribe to given Events objects.
    async fn jobs_subscribe_to_events(
        &self,
        events: &[Event],
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        let jobs = self.jobs()?;
        let filter = FilterBuilder::default().address(vec![jobs.address()]);

        /*
        for evt in events.iter() {
            filter = filter.topics(Some(vec![evt.signature()]), None, None, None);
        }
        */

        let filter = filter.topics(
            Some(events.iter().map(|e| e.signature()).collect()),
            None,
            None,
            None,
        );

        let stream_fut =
            Compat01As03::new(self.web3.eth_subscribe().subscribe_logs(filter.build()));
        let stream = Compat01As03::new(stream_fut.await?);

        let jobs_ethabi = self.jobs_ethabi()?;
        Ok(stream.map(move |e| match e {
            Ok(log) => (&jobs_ethabi, log).try_into(),
            Err(e) => Err(Error::Web3(e)),
        }))
    }

    /// Subscribe to given list of events kinds.
    pub async fn jobs_subscribe_to_event_kinds(
        &self,
        events: &[JobsEventKind],
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        let mut list = Vec::with_capacity(events.len());
        for evt in events {
            list.push(self.jobs_event(*evt)?);
        }

        self.jobs_subscribe_to_events(&list[..]).await
    }

    /// Subscribe to given event kind.
    pub async fn jobs_subscribe_to_event_kind(
        &self,
        event: JobsEventKind,
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        let evt = self.jobs_event(event)?;
        let array: [Event; 1] = [evt];

        self.jobs_subscribe_to_events(&array[..]).await
    }

    pub fn jobs_events(&self) -> Result<Vec<ethabi::Event>, Error> {
        let c = self.jobs_ethabi()?;
        Ok(c.events().cloned().collect())
    }

    fn jobs_event(&self, event: JobsEventKind) -> Result<Event, Error> {
        Ok(self
            .jobs_ethabi()?
            .event(&format!("{}", event)[..])?
            .clone())
    }

    pub async fn jobs_new_draft(&self, job: &Job) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        match job.program_kind {
            ProgramKind::Wasm0m1n0 => {
                let stream = self
                    .jobs_subscribe_to_event_kind(JobsEventKind::JobNew)
                    .await?;

                let fut =
                    jobs.call_with_confirmations("create_job", (), addr, Default::default(), 0);
                Compat01As03::new(fut).await?;

                // TODO: concurrency issues ?
                // TODO: We have to suppose the user hasn't created another job at the same time.
                // TODO: check jobs value ?
                let nonce = stream
                    .filter_map(|e| {
                        future::ready(match e {
                            Ok(JobsEvent::JobNew { sender, nonce }) if sender == addr => {
                                Some(nonce)
                            }
                            _ => None,
                        })
                    })
                    .next()
                    .await
                    .ok_or(Error::CouldntFindJobNonceEvent)?;

                let fut = jobs.call_with_confirmations(
                    "set_parameters_draft",
                    (nonce, job.timeout, job.max_failures, job.redundancy),
                    addr,
                    Default::default(),
                    0,
                );
                Compat01As03::new(fut).await?;

                let other_data = job.other_data();
                let mut encoded_data = Vec::with_capacity(other_data.encoded_len());
                other_data.encode(&mut encoded_data)?;
                let fut = jobs.call_with_confirmations(
                    "set_data_draft",
                    (nonce, encoded_data),
                    addr,
                    Default::default(),
                    0,
                );
                Compat01As03::new(fut).await?;

                let fut = jobs.call_with_confirmations(
                    "set_arguments_draft",
                    (nonce, job.arguments.clone()),
                    addr,
                    Default::default(),
                    0,
                );
                Compat01As03::new(fut).await?;

                let fut = jobs.call_with_confirmations(
                    "set_worker_parameters_draft",
                    (
                        nonce,
                        job.max_worker_price,
                        job.max_network_usage,
                        job.max_network_price,
                    ),
                    addr,
                    Default::default(),
                    0,
                );
                Compat01As03::new(fut).await?;

                Ok(nonce)
            } /*
              _ => Err(Error::UnsupportedProgramKind(program_kind)),
              */
        }
    }

    pub async fn jobs_get_job(&self, job_id: &JobId) -> Result<Job, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_parameters",
            job_id.as_bytes32(),
            addr,
            Default::default(),
            None,
        );
        let (timeout, max_failures, redundancy): (u64, u64, u64) = Compat01As03::new(fut).await?;

        let fut = jobs.query(
            "get_data",
            job_id.as_bytes32(),
            addr,
            Default::default(),
            None,
        );
        let data: Vec<u8> = Compat01As03::new(fut).await?;
        let data = OtherData::decode(&data[..])?;

        let fut = jobs.query(
            "get_worker_parameters",
            job_id.as_bytes32(),
            addr,
            Default::default(),
            None,
        );
        let (max_worker_price, max_network_usage, max_network_price): (u64, u64, u64) =
            Compat01As03::new(fut).await?;

        let fut = jobs.query(
            "get_sender_nonce",
            job_id.as_bytes32(),
            addr,
            Default::default(),
            None,
        );
        let (sender, nonce): (Address, u128) = Compat01As03::new(fut).await?;

        let fut = jobs.query(
            "get_arguments",
            job_id.as_bytes32(),
            addr,
            Default::default(),
            None,
        );
        let arguments: Vec<Vec<u8>> = Compat01As03::new(fut).await?;

        let mut job = Job::new(
            ProgramKind::from_i32(data.program_kind).ok_or(Error::OtherDataDecodeEnumError)?,
            data.program_addresses,
            Multihash::from_bytes(data.program_hash)?,
            arguments,
            sender,
        );
        job.set_timeout(timeout);
        job.set_max_failures(max_failures);
        job.set_best_method(
            BestMethod::from_i32(data.best_method).ok_or(Error::OtherDataDecodeEnumError)?,
        );
        job.set_max_worker_price(max_worker_price);
        job.set_min_cpu_count(data.min_cpu_count);
        job.set_min_memory(data.min_memory);
        job.set_max_network_usage(max_network_usage);
        job.set_max_network_price(max_network_price);
        job.set_min_network_speed(data.min_network_speed);
        job.set_redundancy(redundancy);
        job.set_is_program_pure(data.is_program_pure);
        job.set_nonce(Some(nonce));

        Ok(job)
    }

    pub async fn jobs_get_draft_job(&self, nonce: u128) -> Result<Job, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_parameters_draft",
            nonce,
            addr,
            Default::default(),
            None,
        );
        let (timeout, max_failures, redundancy): (u64, u64, u64) = Compat01As03::new(fut).await?;

        let fut = jobs.query("get_data_draft", nonce, addr, Default::default(), None);
        let data: Vec<u8> = Compat01As03::new(fut).await?;
        let data = OtherData::decode(&data[..])?;

        let fut = jobs.query(
            "get_worker_parameters_draft",
            nonce,
            addr,
            Default::default(),
            None,
        );
        let (max_worker_price, max_network_usage, max_network_price): (u64, u64, u64) =
            Compat01As03::new(fut).await?;

        let fut = jobs.query("get_arguments_draft", nonce, addr, Default::default(), None);
        let arguments: Vec<Vec<u8>> = Compat01As03::new(fut).await?;

        let mut job = Job::new(
            ProgramKind::from_i32(data.program_kind).ok_or(Error::OtherDataDecodeEnumError)?,
            data.program_addresses,
            Multihash::from_bytes(data.program_hash)?,
            arguments,
            addr,
        );
        job.set_timeout(timeout);
        job.set_max_failures(max_failures);
        job.set_best_method(
            BestMethod::from_i32(data.best_method).ok_or(Error::OtherDataDecodeEnumError)?,
        );
        job.set_max_worker_price(max_worker_price);
        job.set_min_cpu_count(data.min_cpu_count);
        job.set_min_memory(data.min_memory);
        job.set_max_network_usage(max_network_usage);
        job.set_max_network_price(max_network_price);
        job.set_min_network_speed(data.min_network_speed);
        job.set_redundancy(redundancy);
        job.set_is_program_pure(data.is_program_pure);
        job.set_nonce(Some(nonce));

        Ok(job)
    }

    pub async fn jobs_get_pending_jobs(&self) -> Result<Vec<JobId>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_pending_jobs", (), addr, Default::default(), None);
        let mut pending_jobs: Vec<[u8; 32]> = Compat01As03::new(fut).await?;

        Ok(pending_jobs.drain(..).map(JobId::from).collect())
    }

    pub async fn jobs_get_draft_jobs(&self) -> Result<Vec<u128>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_draft_jobs", (), addr, Default::default(), None);
        let pending_jobs: Vec<u128> = Compat01As03::new(fut).await?;

        Ok(pending_jobs)
    }

    pub async fn jobs_delete_draft_job(
        &self,
        nonce: u128,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut =
            jobs.call_with_confirmations("delete_draft_nonce", nonce, addr, Default::default(), 0);
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_get_result(&self, task_id: &TaskId) -> Result<Vec<u8>, Error> {
        let jobs = self.jobs()?;
        let task_id = Vec::from(task_id.as_bytes());
        let addr = self.local_address()?;

        let fut = jobs.query("get_result", task_id, addr, Default::default(), None);
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_set_result(
        &self,
        task_id: &TaskId,
        result: &[u8],
        workers_infos: &[(Address, u64, u64)],
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let task_id = Vec::from(task_id.as_bytes());
        let addr = self.local_address()?;

        let mut workers = Vec::new();
        let mut worker_prices = Vec::new();
        let mut network_prices = Vec::new();

        workers_infos.iter().for_each(|(w, p, n)| {
            workers.push(w.clone());
            worker_prices.push(*p);
            network_prices.push(*n);
        });

        let fut = jobs.call_with_confirmations(
            "set_result",
            (
                task_id,
                Vec::from(result),
                workers,
                worker_prices,
                network_prices,
            ),
            addr,
            Default::default(),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_get_pending_locked_money(&self) -> Result<(types::U256, types::U256), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_pending_locked_money",
            (),
            addr,
            Default::default(),
            None,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_send_pending_money(
        &self,
        amount: types::U256,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let local = self.local_balance().await?;
        if local < amount {
            return Err(Error::NotEnoughMoneyInLocalAccount);
        }

        let fut = jobs.call_with_confirmations(
            "send_pending_money",
            (),
            addr,
            Options::with(|o| o.value = Some(amount)),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_recover_pending_money(
        &self,
        amount: types::U256,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let (pending, _) = self.jobs_get_pending_locked_money().await?;
        if pending < amount {
            return Err(Error::NotEnoughMoneyInPending);
        }

        let fut = jobs.call_with_confirmations(
            "recover_pending_money",
            amount,
            addr,
            Default::default(),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_ready(&self, nonce: u128) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let job = self.jobs_get_draft_job(nonce).await?;
        if !job.is_complete() {
            return Err(Error::JobNotComplete);
        }

        let (pending, _): (types::U256, _) = self.jobs_get_pending_locked_money().await?;
        if pending.low_u128() < job.calc_max_price() as u128 {
            return Err(Error::NotEnoughMoneyInPending);
        }

        let fut = jobs.call_with_confirmations("ready", nonce, addr, Default::default(), 0);
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_validate_results(
        &self,
        job_id: &JobId,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call_with_confirmations(
            "validate_results",
            job_id.as_bytes32(),
            addr,
            Default::default(),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_get_task(&self, task_id: &TaskId) -> Result<(JobId, u128, Vec<u8>), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_task",
            task_id.as_bytes32(),
            addr,
            Default::default(),
            None,
        );
        let (job_id, argument_id, argument): (Vec<u8>, u128, Vec<u8>) =
            Compat01As03::new(fut).await?;
        Ok((job_id.as_slice().try_into()?, argument_id, argument))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
