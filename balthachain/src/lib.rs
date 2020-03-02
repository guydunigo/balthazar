extern crate balthamisc as misc;
extern crate ethabi;
extern crate futures;
extern crate web3;

mod config;
pub use config::{Address, ChainConfig};
use ethabi::Event;
use futures::{compat::Compat01As03, executor::block_on, future, Stream, StreamExt};
use misc::{
    job::{BestMethod, Job, JobId, ProgramKind, TaskId, UnknownValue, WorkerParameters},
    multiaddr::{self, Multiaddr},
    multiformats::encode_multibase_multihash_string,
    multihash::{self, Multihash},
};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use web3::{
    contract::{Contract, Error as ContractError},
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
    /// When storing a job, an JobNew event is sent with the new job id.
    /// This error is sent when the event couldn't be found.
    CouldntFindJobIdEvent,
    MultiaddrParse(multiaddr::Error),
    Multihash(multihash::DecodeOwnedError),
    ProgramKindParse(UnknownValue<(u64, Option<Multihash>)>),
    BestMethodParse(UnknownValue<u64>),
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

impl From<multihash::DecodeOwnedError> for Error {
    fn from(e: multihash::DecodeOwnedError) -> Self {
        Error::Multihash(e)
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
    JobsCounterGet,
    JobsCounterSet(u128),
    JobsCounterInc,
    /// Subscribe to Jobs smart contract given events or if none are provided, all of them.
    JobsSubscribe(Vec<JobsEventKind>),
    /// List all Jobs smart contract events.
    JobsEvents,
    JobsLength,
    JobsSendJob {
        program_kind: ProgramKind,
        addresses: Vec<Multiaddr>,
        arguments: Vec<Vec<u8>>,
        timeout: u64,
        max_failures: u64,
        best_method: BestMethod,
        max_worker_price: u64,
        min_cpu_count: u64,
        min_memory: u64,
        min_network_speed: u64,
        max_network_usage: u64,
        redundancy: u64,
        includes_tests: bool,
    },
    JobsGetJob {
        job_id: JobId,
    },
    JobsSetResult {
        job_id: JobId,
        task_id: TaskId,
        result: Vec<u8>,
    },
    JobsGetResult {
        job_id: JobId,
        task_id: TaskId,
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
                        println!("{:?}", e);
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
        RunMode::JobsLength => {
            let len = chain.jobs_length().await?;
            println!("Number of stored jobs: {}.", len);
        }
        RunMode::JobsSendJob {
            program_kind,
            addresses,
            arguments,
            timeout,
            max_failures,
            best_method,
            max_worker_price,
            min_cpu_count,
            min_memory,
            min_network_speed,
            max_network_usage,
            redundancy,
            includes_tests,
        } => {
            let job = Job {
                job_id: None,
                program_kind: program_kind.clone(),
                addresses: addresses.clone(),
                arguments: arguments.clone(),
                timeout: *timeout,
                max_failures: *max_failures,
                worker_parameters: WorkerParameters {
                    best_method: *best_method,
                    max_worker_price: *max_worker_price,
                    min_cpu_count: *min_cpu_count,
                    min_memory: *min_memory,
                    min_network_speed: *min_network_speed,
                    max_network_usage: *max_network_usage,
                },
                redundancy: *redundancy,
                includes_tests: *includes_tests,
                sender: chain.local_address()?,
            };

            let job_id = chain.jobs_send_job(&job).await?;

            println!(
                "Job stored ! Job id : {}",
                encode_multibase_multihash_string(&job_id)
            );
        }
        RunMode::JobsGetJob { job_id } => {
            let job = chain.jobs_get_job(*job_id).await?;
            println!("{}", job);
        }
        RunMode::JobsGetResult { job_id, task_id } => {
            let result = chain.jobs_get_result(*job_id, *task_id).await?;
            println!(
                "Result of task `{}` of job `{}`:\n{}",
                encode_multibase_multihash_string(task_id),
                encode_multibase_multihash_string(job_id),
                String::from_utf8_lossy(&result[..])
            );
        }
        RunMode::JobsSetResult {
            job_id,
            task_id,
            result,
        } => {
            chain.jobs_set_result(*job_id, *task_id, result).await?;
            println!(
                "Result of task `{}` of job `{}` stored.",
                encode_multibase_multihash_string(task_id),
                encode_multibase_multihash_string(job_id)
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub enum JobsEvent {
    JobNew {
        sender: Address,
        job_id: u64,
    },
    JobLocked {
        job_id: u64,
    },
    TaskNewResult {
        job_id: JobId,
        task_id: TaskId,
        result: Vec<u8>,
    },
    CounterHasNewValue(u128),
}

impl fmt::Display for JobsEvent {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsEvent::TaskNewResult {
                job_id,
                task_id,
                result,
            } => write!(
                fmt,
                "TaskNewResult {{ job_id: {}, task_id: {}, result: {} }}",
                job_id,
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
                    JobsEventKind::JobNew => {
                        let len = log.data.0.len();
                        if len == 64 {
                            let sender = types::Address::from_slice(&log.data.0[(32 - 20)..32]);
                            let job_id =
                                types::U64::from_big_endian(&log.data.0[(64 - 8)..64]).as_u64();
                            return Ok(JobsEvent::JobNew { sender, job_id });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected: 64,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::JobLocked => {
                        let len = log.data.0.len();
                        if len == 32 {
                            let job_id =
                                types::U64::from_big_endian(&log.data.0[(32 - 8)..32]).as_u64();
                            return Ok(JobsEvent::JobLocked { job_id });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected: 32,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
                    JobsEventKind::TaskNewResult => {
                        let len = log.data.0.len();
                        if len > 64 {
                            let job_id =
                                types::U64::from_big_endian(&log.data.0[(32 - 8)..32]).as_u64();
                            let task_id =
                                types::U64::from_big_endian(&log.data.0[(64 - 8)..64]).as_u64();
                            return Ok(JobsEvent::TaskNewResult {
                                job_id,
                                task_id,
                                result: Vec::from(&log.data.0[64..]),
                            });
                        } else {
                            return Err(Error::JobsEventDataWrongSize {
                                expected: 64,
                                got: len,
                                data: log.data.0,
                            });
                        }
                    }
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
                }
            }
        }

        Err(Error::CouldntParseJobsEventFromLog(Box::new(log)))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JobsEventKind {
    JobNew,
    JobLocked,
    TaskNewResult,
    CounterHasNewValue,
}

impl std::convert::TryFrom<&str> for JobsEventKind {
    type Error = Error;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        match src {
            "JobNew" => Ok(JobsEventKind::JobNew),
            "JobLocked" => Ok(JobsEventKind::JobLocked),
            "TaskNewResult" => Ok(JobsEventKind::TaskNewResult),
            "CounterHasNewValue" => Ok(JobsEventKind::CounterHasNewValue),
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

    pub async fn jobs_length(&self) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_jobs_length", (), addr, Default::default(), None);
        Ok(Compat01As03::new(fut).await?)
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

    pub async fn jobs_send_job(&self, job: &Job<Address>) -> Result<JobId, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;
        let best_method: u64 = job.worker_parameters.best_method.into();

        match job.program_kind {
            ProgramKind::Wasm(ref hash) => {
                let stream = self
                    .jobs_subscribe_to_event_kind(JobsEventKind::JobNew)
                    .await?;

                let args = (
                    hash.clone().into_bytes(),
                    job.timeout,
                    job.max_failures,
                    best_method,
                    job.worker_parameters.max_worker_price,
                    job.worker_parameters.min_cpu_count,
                    job.worker_parameters.min_memory,
                    job.worker_parameters.min_network_speed,
                    job.worker_parameters.max_network_usage,
                    job.redundancy,
                );
                let fut = jobs.call_with_confirmations(
                    "send_wasm_job",
                    args,
                    addr,
                    Default::default(),
                    0,
                );

                Compat01As03::new(fut).await?;
                // TODO: concurrency issues ?
                // TODO: We have to suppose the user hasn't created another job at the same time.
                // TODO: check jobs value ?
                let job_id = stream
                    .filter_map(|e| {
                        future::ready(match e {
                            Ok(JobsEvent::JobNew { sender, job_id }) if sender == addr => {
                                Some(job_id)
                            }
                            _ => None,
                        })
                    })
                    .next()
                    .await
                    .ok_or(Error::CouldntFindJobIdEvent)?;

                let fut = jobs.call_with_confirmations(
                    "set_includes_tests",
                    (job_id, job.includes_tests),
                    addr,
                    Default::default(),
                    0,
                );
                Compat01As03::new(fut).await?;

                for address in job.addresses.iter() {
                    let fut = jobs.call_with_confirmations(
                        "push_program_address",
                        (job_id, address.clone().into_bytes()),
                        addr,
                        Default::default(),
                        0,
                    );
                    Compat01As03::new(fut).await?;
                }

                for args in job.arguments.iter() {
                    let fut = jobs.call_with_confirmations(
                        "push_program_arguments",
                        (job_id, args.clone()),
                        addr,
                        Default::default(),
                        0,
                    );
                    Compat01As03::new(fut).await?;
                }

                let fut = jobs.call_with_confirmations("lock", job_id, addr, Default::default(), 0);
                Compat01As03::new(fut).await?;

                Ok(job_id)
            } /*
              _ => Err(Error::UnsupportedProgramKind(program_kind)),
              */
        }
    }

    pub async fn jobs_get_job(&self, job_id: JobId) -> Result<Job<Address>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_job", job_id, addr, Default::default(), None);
        let (program_kind, program_hash, timeout, max_failures, redundancy, includes_tests, sender): (u64, Vec<u8>, u64, u64, u64, bool, Address) =
            Compat01As03::new(fut).await?;
        let program_hash = Multihash::from_bytes(program_hash)?;

        let fut = jobs.query(
            "get_worker_parameters",
            job_id,
            addr,
            Default::default(),
            None,
        );
        let (
            best_method,
            max_worker_price,
            min_cpu_count,
            min_memory,
            min_network_speed,
            max_network_usage,
        ): (u64, u64, u64, u64, u64, u64) = Compat01As03::new(fut).await?;

        let addresses = self.jobs_get_addresses(job_id).await?;
        let arguments = self.jobs_get_all_arguments(job_id).await?;

        let job = Job {
            job_id: Some(job_id),
            program_kind: (program_kind, Some(program_hash))
                .try_into()
                .map_err(Error::ProgramKindParse)?,
            addresses,
            arguments,
            timeout,
            max_failures,
            worker_parameters: WorkerParameters {
                best_method: best_method.try_into().map_err(Error::BestMethodParse)?,
                max_worker_price,
                min_cpu_count,
                min_memory,
                min_network_speed,
                max_network_usage,
            },
            redundancy,
            includes_tests,
            sender,
        };

        Ok(job)
    }

    pub async fn jobs_get_addresses(&self, job_id: JobId) -> Result<Vec<Multiaddr>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_addresses", job_id, addr, Default::default(), None);
        let addresses_raw: Vec<Vec<u8>> = Compat01As03::new(fut).await?;
        let mut addresses: Vec<Multiaddr> = Vec::with_capacity(addresses_raw.len());
        for addr in addresses_raw.iter() {
            addresses.push(Multiaddr::from_bytes(addr.clone())?);
        }

        Ok(addresses)
    }

    pub async fn jobs_get_all_arguments(&self, job_id: JobId) -> Result<Vec<Vec<u8>>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_arguments_length",
            job_id,
            addr,
            Default::default(),
            None,
        );
        let nb_tasks: u64 = Compat01As03::new(fut).await?;

        let mut arguments = Vec::with_capacity(nb_tasks as usize);
        for i in 0..nb_tasks {
            arguments.push(self.jobs_get_arguments(job_id, i).await?);
        }

        Ok(arguments)
    }

    pub async fn jobs_get_arguments(
        &self,
        job_id: JobId,
        task_id: TaskId,
    ) -> Result<Vec<u8>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_arguments",
            (job_id, task_id),
            addr,
            Default::default(),
            None,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_get_result(&self, job_id: JobId, task_id: TaskId) -> Result<Vec<u8>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_result",
            (job_id, task_id),
            addr,
            Default::default(),
            None,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_set_result(
        &self,
        job_id: JobId,
        task_id: TaskId,
        result: &[u8],
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call_with_confirmations(
            "set_result",
            (job_id, task_id, Vec::from(result)),
            addr,
            Default::default(),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
