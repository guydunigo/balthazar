extern crate balthamisc as misc;
extern crate ethabi;
extern crate futures;
extern crate web3;

mod config;
pub use config::{Address, ChainConfig};
use ethabi::Event;
use futures::{compat::Compat01As03, executor::block_on, future, Stream, StreamExt};
use misc::job::{BestMethod, JobId, ProgramKind, TaskId};
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
    JobsSubscribe,
    JobsEvents,
    JobsSendJob {
        program_kind: ProgramKind,
        addresses: Vec<Vec<u8>>,
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
    JobsLength,
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
        RunMode::JobsSubscribe => {
            let stream = chain.jobs_subscribe().await?;

            stream
                .for_each(|e| {
                    println!("{:?}", e);
                    future::ready(())
                })
                .await;
        }
        RunMode::JobsEvents => {
            for evt in chain.jobs_events()? {
                println!("{:?}, {}", evt, evt.signature());
            }
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
            let job_id = chain
                .jobs_send_job(
                    program_kind,
                    &addresses.iter().map(|a| &a[..]).collect::<Vec<&[u8]>>()[..],
                    &arguments.iter().map(|a| &a[..]).collect::<Vec<&[u8]>>()[..],
                    *timeout,
                    *max_failures,
                    *best_method,
                    *max_worker_price,
                    *min_cpu_count,
                    *min_memory,
                    *min_network_speed,
                    *max_network_usage,
                    *redundancy,
                    *includes_tests,
                )
                .await?;

            println!("Job stored ! Job id : {}", job_id);
        }
        RunMode::JobsLength => {
            let len = chain.jobs_length().await?;
            println!("Number of stored jobs: {}.", len);
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
                        if len == 96 {
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
                                expected: 96,
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

    pub async fn jobs_set_counter(&self, new: u128) -> Result<types::H256, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call("set_counter", new, addr, Default::default());
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_inc_counter(&self) -> Result<types::H256, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call("inc_counter", (), addr, Default::default());
        Ok(Compat01As03::new(fut).await?)
    }

    /// Substribe to events.
    // TODO: convert events and parameters to local enum.
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

    pub fn jobs_events(&self) -> Result<Vec<ethabi::Event>, Error> {
        let c = self.jobs_ethabi()?;
        Ok(c.events().cloned().collect())
    }

    fn jobs_get_event(&self, event: JobsEventKind) -> Result<Event, Error> {
        Ok(self
            .jobs_ethabi()?
            .event(&format!("{}", event)[..])?
            .clone())
    }

    /*
    fn jobs_event_once(&self, event: JobsEventKind) -> Result<JobsEventKind, Error> {
        let evt = self.jobs_get_event(event)?;
        self.web3.eth_subscribe().subscribe_logs
    }
    */

    pub async fn jobs_send_job(
        &self,
        program_kind: &ProgramKind,
        addresses: &[&[u8]],
        arguments: &[&[u8]],
        timeout: u64,
        max_failures: u64,
        best_method: BestMethod,
        max_worker_price: u64,
        min_cpu_count: u64,
        min_memory: u64,
        min_network_speed: u64,
        max_network_usage: u64,
        redundancy: u64,
        _includes_tests: bool,
    ) -> Result<JobId, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;
        let best_method: u64 = best_method.into();

        match program_kind {
            ProgramKind::Wasm(hash) => {
                let args = (
                    hash.clone().into_bytes(),
                    timeout,
                    max_failures,
                    best_method,
                    max_worker_price,
                    min_cpu_count,
                    min_memory,
                    min_network_speed,
                    max_network_usage,
                    redundancy,
                );
                let fut = jobs.call_with_confirmations(
                    "send_wasm_job",
                    args,
                    addr,
                    Default::default(),
                    0,
                );

                Compat01As03::new(fut).await?;
                let job_id = 0;
                println!("2, {}", job_id);

                for address in addresses.iter() {
                    let fut = jobs.call_with_confirmations(
                        "push_program_address",
                        (job_id, Vec::from(*address)),
                        addr,
                        Default::default(),
                        0,
                    );
                    Compat01As03::new(fut).await?;
                }

                for args in arguments.iter() {
                    let fut = jobs.call_with_confirmations(
                        "push_program_arguments",
                        (job_id, Vec::from(*args)),
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
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
