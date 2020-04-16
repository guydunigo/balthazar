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
// mod run;
// pub use run::{run, RunMode};

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
fn try_convert_task_error_kind(reason_nb: u64) -> Option<TaskErrorKind> {
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

/// State a task can be on the blockchain.
pub enum JobsTaskState {
    /// The task can still be executed and all.
    Incomplete,
    /// The task was successfuly computed and here is the selected result.
    /// All involved parties are paid and the sender has been refunded from the
    /// remaining money for this task.
    Completed(Vec<u8>),
    /// The task is definetely failed and won't be scheduled again.
    DefinetelyFailed(TaskErrorKind),
}

fn try_convert_tasks_state(
    state_nb: u64,
    result: Vec<u8>,
    reason: TaskErrorKind,
) -> Option<JobsTaskState> {
    match state_nb {
        0 => Some(JobsTaskState::Incomplete),
        1 => Some(JobsTaskState::Completed(result)),
        2 => Some(JobsTaskState::DefinetelyFailed(reason)),
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

    /// Return the address of the our account on the blockchain.
    pub fn local_address(&self) -> Result<&Address, Error> {
        if let Some(addr) = self.config.ethereum_address() {
            Ok(addr)
        } else {
            Err(Error::MissingLocalAddress)
        }
    }

    /// Get information about given block.
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

    /// Get balance of local account if [`ChainConfig::ethereum_address`] is `Some(_)`.
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
    fn jobs_ethabi(&self) -> Result<ethabi::Contract, Error> {
        if let Some((_, abi)) = self.config.contract_jobs() {
            let c = ethabi::Contract::load(&abi[..])?;
            Ok(c)
        } else {
            Err(Error::MissingJobsContractData)
        }
    }

    /*
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
    */

    /// Subscribe to all events on given contract.
    pub async fn jobs_subscribe(
        &self,
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        self.jobs_subscribe_to_events(&[][..]).await
    }

    /// Subscribe to given Events objects.
    async fn jobs_subscribe_to_events(
        &self,
        events: &[Event],
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        let jobs = self.jobs()?;
        let filter = FilterBuilder::default().address(vec![jobs.address()]);

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

    // TODO: useful ?
    /// Subscribe to given event kind.
    pub async fn jobs_subscribe_to_event_kind(
        &self,
        event: JobsEventKind,
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        self.jobs_subscribe_to_events(&[self.jobs_event(event)?][..])
            .await
    }

    /// Get a list of all events from the smart-contract's ABI format.
    pub fn jobs_events(&self) -> Result<Vec<ethabi::Event>, Error> {
        let c = self.jobs_ethabi()?;
        Ok(c.events().cloned().collect())
    }

    /// Get event from the smart-contract's ABI corresponding to given [`JobsEventKind`].
    fn jobs_event(&self, event: JobsEventKind) -> Result<Event, Error> {
        Ok(self
            .jobs_ethabi()?
            .event(&format!("{}", event)[..])?
            .clone())
    }

    /// Get the amount of money the Jobs SC contains for the local account in
    /// the pending account and the locked one.
    // TODO: not possible from someone else, yet.
    pub async fn jobs_get_pending_locked_money_local(
        &self,
    ) -> Result<(types::U256, types::U256), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "get_pending_locked_money",
            (),
            *addr,
            Default::default(),
            None,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    ///  Send money to local address's pending account.
    // TODO: not possible from someone else, yet.
    async fn jobs_send_pending_money_local(
        &self,
        amount: types::U256,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let local = self.local_balance().await?;
        if local < amount {
            Err(Error::NotEnoughMoneyInAccount(*addr, local))
        } else {
            let fut = jobs.call_with_confirmations(
                "send_pending_money",
                (),
                *addr,
                Options::with(|o| o.value = Some(amount)),
                0,
            );
            Ok(Compat01As03::new(fut).await?)
        }
        // TODO: check new values
    }

    /// Recover money from the pending account associated to local address.
    ///
    /// > **Note:** Only available for owner of the account.
    pub async fn jobs_recover_pending_money(
        &self,
        amount: types::U256,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let (pending, _) = self.jobs_get_pending_locked_money_local().await?;
        if pending < amount {
            return Err(Error::NotEnoughMoneyInPending);
        }

        let fut = jobs.call_with_confirmations(
            "recover_pending_money",
            amount,
            *addr,
            Default::default(),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
        // TODO: check new values
    }

    /// Get next nonce used when a new draft job will be created.
    /// This means thas every nonce strictly inferior to it may refer to an existing job.
    pub async fn jobs_get_next_nonce(&self) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_next_nonce", (), *addr, Default::default(), None);
        Ok(Compat01As03::new(fut).await?)
    }

    /// Creates a new draft job based on given job, and return it's [`JobId`].
    /// Every data from the given job is set in the blockchain.
    ///
    /// The job won't be executed at once, there still needs to be enough money in
    /// the local address's pending account (e.g. using [`jobs_send_pending_money_local`]),
    /// and mark it as ready using [`jobs_ready`].
    // TODO: check new values
    // TODO: split in sub-functions ?
    pub async fn jobs_create_draft(&self, job: &Job) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let stream = self
            .jobs_subscribe_to_event_kind(JobsEventKind::JobNew)
            .await?;

        let fut = jobs.call_with_confirmations("create_draft", (), *addr, Default::default(), 0);
        Compat01As03::new(fut).await?;

        // TODO: concurrency issues ?
        // TODO: We have to suppose the user hasn't created another job at the same time.
        // TODO: or use [`get_next_nonce`]?
        // TODO: check jobs values ?
        let nonce = stream
            .filter_map(|e| {
                future::ready(match e {
                    Ok(JobsEvent::JobNew {
                        sender: addr,
                        nonce,
                    }) => Some(nonce),
                    _ => None,
                })
            })
            .next()
            .await
            .ok_or(Error::CouldntFindJobNewEvent)?;
        let job_id_32 = JobId::job_id(addr, nonce).as_bytes32();

        let fut = jobs.call_with_confirmations(
            "set_parameters",
            (
                job_id_32,
                job.timeout(),
                job.max_failures(),
                job.redundancy(),
            ),
            *addr,
            Default::default(),
            0,
        );
        Compat01As03::new(fut).await?;

        let other_data = job.other_data();
        let mut encoded_data = Vec::with_capacity(other_data.encoded_len());
        other_data.encode(&mut encoded_data)?;
        let fut = jobs.call_with_confirmations(
            "set_data",
            (job_id_32, encoded_data),
            *addr,
            Default::default(),
            0,
        );
        Compat01As03::new(fut).await?;

        let fut = jobs.call_with_confirmations(
            "set_arguments",
            (job_id_32, job.arguments().clone()),
            *addr,
            Default::default(),
            0,
        );
        Compat01As03::new(fut).await?;

        let fut = jobs.call_with_confirmations(
            "set_worker_parameters",
            (
                job_id_32,
                job.max_worker_price(),
                job.max_network_usage(),
                job.max_network_price(),
            ),
            *addr,
            Default::default(),
            0,
        );
        Compat01As03::new(fut).await?;

        let fut = jobs.call_with_confirmations(
            "set_management_parameters",
            (
                job_id_32,
                job.min_checking_interval(),
                job.management_price(),
            ),
            *addr,
            Default::default(),
            0,
        );
        Compat01As03::new(fut).await?;

        Ok(nonce)
    }

    /// Deletes a draft job given it exists, belongs to the local address, and is a draft.
    pub async fn jobs_delete_draft(
        &self,
        job_id: &JobId,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let (sender, _) = self.jobs_get_sender_nonce(job_id, true).await?;
        if sender == *addr {
            return Err(Error::JobNotOurs(job_id.clone()));
        }
        if !self.jobs_is_draft(job_id, false).await? {
            return Err(Error::JobNotADraft(job_id.clone()));
        }

        let fut = jobs.call_with_confirmations(
            "delete_draft",
            job_id.as_bytes32(),
            *addr,
            Default::default(),
            0,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    /// Locks a job from draft to pending so it can be computed.
    /// The maximum amount of money it can need will be locked as well as all
    /// its parameters.
    pub async fn jobs_lock(&self, job_id: &JobId) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let job = self.jobs_get_job(job_id, true).await?;
        if job.sender() == *addr {
            return Err(Error::JobNotOurs(job_id.clone()));
        }

        if !job.is_ready() {
            return Err(Error::JobNotReady(job_id.clone()));
        }

        let (pending, _): (types::U256, _) = self.jobs_get_pending_locked_money_local().await?;
        if pending.low_u128() < job.calc_max_price() as u128 {
            return Err(Error::NotEnoughMoneyInPending);
        }

        let fut =
            jobs.call_with_confirmations("lock", job_id.as_bytes32(), *addr, Default::default(), 0);
        Ok(Compat01As03::new(fut).await?)
    }

    // TODO
    pub async fn jobs_get_result(&self, task_id: &TaskId) -> Result<Vec<u8>, Error> {
        let jobs = self.jobs()?;
        let task_id = Vec::from(task_id.as_bytes());
        let addr = self.local_address()?;

        let fut = jobs.query("get_result", task_id, addr, Default::default(), None);
        Ok(Compat01As03::new(fut).await?)
    }

    // TODO
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

    /// Get [`Job::timeout`], [`Job::redundancy`] and [`Job::max_failures`] for given job.
    pub async fn jobs_get_parameters(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<(u64, u64, u64), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "get_parameters",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(Compat01As03::new(fut).await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get the data in [`OtherData`] for given job.
    pub async fn jobs_get_other_data(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<OtherData, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "get_other_data",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            let other_data_bytes: Vec<u8> = Compat01As03::new(fut).await?;
            let other_data = OtherData::decode(&other_data_bytes[..])?;
            Ok(other_data)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get [`Job::arguments`] for given job.
    pub async fn jobs_get_arguments(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "get_arguments",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(Compat01As03::new(fut).await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get [`Job::max_worker_price`], [`Job::max_network_usage`],
    /// and [`Job::max_network_price`] for given job.
    pub async fn jobs_get_worker_parameters(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<(u64, u64, u64), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "get_worker_parameters",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(Compat01As03::new(fut).await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get [`Job::min_checking_interval`] and [`Job::management_price`] for given job.
    pub async fn jobs_get_management_parameters(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<(u64, u64), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "get_management_parameters",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(Compat01As03::new(fut).await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get [`Job::sender`] and [`Job::nonce`] for given job.
    pub async fn jobs_get_sender_nonce(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<(Address, u128), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "get_sender_nonce",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(Compat01As03::new(fut).await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Check if a job id corresponds to an existing job.
    pub async fn jobs_is_non_null(&self, job_id: &JobId) -> Result<bool, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "is_non_null",
            job_id.as_bytes32(),
            *addr,
            Default::default(),
            None,
        );
        Ok(Compat01As03::new(fut).await?)
    }

    /// Check if the job is a draft and can still be modified by its sender.
    pub async fn jobs_is_draft(&self, job_id: &JobId, check_non_null: bool) -> Result<bool, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_non_null(job_id).await? {
            let fut = jobs.query(
                "is_draft",
                job_id.as_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(Compat01As03::new(fut).await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get a job from the blockchain.
    pub async fn jobs_get_job(&self, job_id: &JobId, check_non_null: bool) -> Result<Job, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let (timeout, redundancy, max_failures) =
            self.jobs_get_parameters(job_id, check_non_null).await?;
        let other_data = self.jobs_get_other_data(job_id, check_non_null).await?;
        let args = self.jobs_get_arguments(job_id, check_non_null).await?;
        let (max_worker_price, max_network_usage, max_network_price) = self
            .jobs_get_worker_parameters(job_id, check_non_null)
            .await?;
        let (min_checking_interval, management_price) = self
            .jobs_get_management_parameters(job_id, check_non_null)
            .await?;
        let (sender, nonce) = self.jobs_get_sender_nonce(job_id, check_non_null).await?;

        let best_method = other_data.best_method();
        let mut job = Job::new(
            other_data.program_kind(),
            other_data.program_addresses,
            Multihash::from_bytes(other_data.program_hash)?,
            args,
            sender,
        );
        job.set_timeout(timeout);
        job.set_max_worker_price(max_worker_price);
        job.set_max_network_usage(max_network_usage);
        job.set_max_network_price(max_network_price);
        job.set_min_checking_interval(min_checking_interval);
        job.set_management_price(management_price);
        job.set_redundancy(redundancy);
        job.set_max_failures(max_failures);
        job.set_best_method(best_method);
        job.set_min_cpu_count(other_data.min_cpu_count);
        job.set_min_memory(other_data.min_memory);
        job.set_min_network_speed(other_data.min_network_speed);
        job.set_is_program_pure(other_data.is_program_pure);
        job.set_nonce(Some(nonce));

        Ok(job)
    }

    /// Among all the jobs created by the local address, find and list all which are
    /// drafts.
    pub async fn jobs_get_draft_jobs_local(&self) -> Result<Vec<JobId>, Error> {
        // TODO
        unimplemented!();
    }

    /// Among all the jobs created by the local address, find and list all which still
    /// have tasks waiting for computation.
    pub async fn jobs_get_pending_jobs_local(&self) -> Result<Vec<JobId>, Error> {
        // TODO
        unimplemented!();
    }

    /// Among all the jobs created by the local address, find and list all which are
    /// completed or definitely failed for all tasks.
    pub async fn jobs_get_completed_jobs_local(&self) -> Result<Vec<JobId>, Error> {
        // TODO
        unimplemented!();
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

// TODO: get list of drafts
// TODO: get list of incomplete jobs
// TODO: get list of all jobs
// TODO: ...
// TODO: check if sender is correct

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
