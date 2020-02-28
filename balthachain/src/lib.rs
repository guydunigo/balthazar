extern crate balthamisc as misc;
extern crate ethabi;
extern crate futures;
extern crate web3;

mod config;
pub use config::{Address, ChainConfig};
use futures::{compat::Compat01As03, executor::block_on, future, Stream, StreamExt};
use misc::job::{BestMethod, JobId, ProgramKind};
pub use web3::types::{Block, BlockId, BlockNumber, H256, U256};
use web3::{
    contract::{Contract, Error as ContractError},
    transports::{EventLoopHandle, WebSocket},
    types::{FilterBuilder, Log},
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
    }

    Ok(())
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

    pub async fn block(&self, block_id: BlockId) -> Result<Option<Block<H256>>, Error> {
        Compat01As03::new(self.web3.eth().block(block_id))
            .await
            .map_err(Error::Web3)
    }

    /// Get balance of given account.
    pub async fn balance(&self, addr: Address) -> Result<U256, Error> {
        Compat01As03::new(self.web3.eth().balance(addr, None))
            .await
            .map_err(Error::Web3)
    }

    /// Get balance if provided account if [`ChainConfig::ethereum_address`] is `Some(_)`.
    pub async fn local_balance(&self) -> Result<U256, Error> {
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

    pub async fn jobs_set_counter(&self, new: u128) -> Result<H256, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call("set_counter", new, addr, Default::default());
        Ok(Compat01As03::new(fut).await?)
    }

    pub async fn jobs_inc_counter(&self) -> Result<H256, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call("inc_counter", (), addr, Default::default());
        Ok(Compat01As03::new(fut).await?)
    }

    /// Substribe to events.
    // TODO: convert events and parameters to local enum.
    pub async fn jobs_subscribe(&self) -> Result<impl Stream<Item = Result<Log, Error>>, Error> {
        let jobs = self.jobs()?;
        let filter = FilterBuilder::default()
            .address(vec![jobs.address()])
            .build();

        let stream_fut = Compat01As03::new(self.web3.eth_subscribe().subscribe_logs(filter));
        Ok(Compat01As03::new(stream_fut.await?).map(|e| e.map_err(Error::Web3)))
    }

    pub fn jobs_events(&self) -> Result<Vec<ethabi::Event>, Error> {
        let c = self.jobs_ethabi()?;
        Ok(c.events().cloned().collect())
    }

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
                let fut = jobs.query("send_wasm_job", args, addr, Default::default(), None);

                let job_id = Compat01As03::new(fut).await?;

                for address in addresses.iter() {
                    let fut = jobs.call(
                        "push_program_address",
                        Vec::from(*address),
                        addr,
                        Default::default(),
                    );
                    Compat01As03::new(fut).await?;
                }

                for args in arguments.iter() {
                    let fut = jobs.call(
                        "push_program_arguments",
                        Vec::from(*args),
                        addr,
                        Default::default(),
                    );
                    Compat01As03::new(fut).await?;
                }

                let fut = jobs.call("lock", (), addr, Default::default());
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
