extern crate balthamisc as misc;
extern crate balthaproto as proto;
extern crate ethabi;
extern crate futures;
extern crate web3;

mod jobs_events;
pub use jobs_events::{JobsEvent, JobsEventKind};
mod error;
pub use error::Error;
mod config;
pub use config::ChainConfig;
mod run;
pub use run::{run, RunMode};

use ethabi::Event;
use futures::{compat::Compat01As03, future, Stream, StreamExt};
use misc::{
    job::{BestMethod, Job, JobId, OtherData, ProgramKind, TaskId},
    multihash::Multihash,
};
use proto::{worker::TaskErrorKind, Message};
use std::convert::TryInto;
use web3::{
    contract::{Contract, Error as ContractError, Options},
    transports::{EventLoopHandle, WebSocket},
    types::{self, Address, Block, BlockId, FilterBuilder},
    Web3,
};

/// Converts the integer value returned by the smart-contract into a [`TaskErrorKind`]
/// enum.
/// Returns [`None`] if the value is unknown.
fn convert_task_error_kind(reason_nb: u64) -> Option<TaskErrorKind> {
    match reason_nb {
        0 => Some(TaskErrorKind::IncorrectSpecification),
        1 => Some(TaskErrorKind::TimedOut),
        2 => Some(TaskErrorKind::Download),
        3 => Some(TaskErrorKind::Runtime),
        4 => Some(TaskErrorKind::IncorrectResult),
        5 => Some(TaskErrorKind::Unknown),
        _ => None,
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
