extern crate balthamisc as misc;
extern crate balthaproto as proto;
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

use web3::ethabi;
use futures::{future, Stream, StreamExt};
use misc::{
    job::{Address, Job, JobId, OtherData, TaskId},
    multihash::Multihash,
    shared_state::WorkerPaymentInfo,
};
use proto::{manager::TaskDefiniteErrorKind, Message};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
};
use web3::{
    contract::{Contract, Error as ContractError, Options},
    transports::WebSocket,
    types::{self, Block, BlockId, FilterBuilder},
    Web3,
};

/// Converts the integer value returned by the smart-contract into a
/// [`TaskDefiniteErrorKind`] enum.
/// Returns [`None`] if the value is unknown.
fn try_convert_task_error_kind(reason_nb: u64) -> Option<TaskDefiniteErrorKind> {
    match reason_nb {
        0 => Some(TaskDefiniteErrorKind::TimedOut),
        1 => Some(TaskDefiniteErrorKind::Download),
        2 => Some(TaskDefiniteErrorKind::Runtime),
        3 => Some(TaskDefiniteErrorKind::IncorrectSpecification),
        4 => Some(TaskDefiniteErrorKind::IncorrectResult),
        _ => None,
    }
}

/// Convert [`TaskDefiniteErrorKind`] to integer for the smart-contract.
/// Returns [`None`] if the error can't be stored on the smart-contract
/// (like [`TaskDefiniteErrorKind::Aborted`]).
fn convert_task_error_kind(reason: TaskDefiniteErrorKind) -> Option<u64> {
    use TaskDefiniteErrorKind::*;
    match reason {
        TimedOut => Some(0),
        Download => Some(1),
        Runtime => Some(2),
        IncorrectSpecification => Some(3),
        IncorrectResult => Some(4),
        // TaskDefiniteErrorKind::Unknown => Some(5),
        Aborted => None,
    }
}

/// State a task can be on the blockchain.
#[derive(Debug, Clone)]
pub enum JobsCompleteness {
    /// The task can still be executed and all.
    Incomplete,
    /// The task was successfuly computed and here is the selected result.
    /// All involved parties are paid and the sender has been refunded from the
    /// remaining money for this task.
    Completed(Vec<u8>),
    /// The task is definetely failed and won't be scheduled again.
    DefinetelyFailed(TaskDefiniteErrorKind),
}

impl fmt::Display for JobsCompleteness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsCompleteness::Completed(res) => {
                write!(f, "Completed({})", String::from_utf8_lossy(res))
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

fn try_convert_tasks_state(
    state_nb: u64,
    result: Vec<u8>,
    reason: TaskDefiniteErrorKind,
) -> Option<JobsCompleteness> {
    match state_nb {
        0 => Some(JobsCompleteness::Incomplete),
        1 => Some(JobsCompleteness::Completed(result)),
        2 => Some(JobsCompleteness::DefinetelyFailed(reason)),
        _ => None,
    }
}

/// Object to communicate with the blockchain and its smart-contracts.
///
/// To avoid unnecessary calls and for better error handling, conditions are checked
/// before any calls to the blockchain.
///
/// > **Note:** Every function modifying a smart-contract will cost money to process.
#[derive(Debug)]
pub struct Chain<'a> {
    web3: Web3<WebSocket>,
    config: &'a ChainConfig,
}

// TODO: explain [`check_non_null`].
// TODO: transaction costs
impl<'a> Chain<'a> {
    pub async fn new(config: &'a ChainConfig) -> Chain<'a> {
        // TODO: handle websocket connection error.
        let transport = WebSocket::new(config.web3_ws()).await.unwrap();
        Chain {
            web3: web3::Web3::new(transport),
            config,
        }
    }

    /// Return the address of the our account on the blockchain,
    /// if given in [`ChainConfig`].
    pub fn local_address(&self) -> Result<&Address, Error> {
        if let Some(addr) = self.config.ethereum_address() {
            Ok(addr)
        } else {
            Err(Error::MissingLocalAddress)
        }
    }

    /// Get information about given block.
    pub async fn block(&self, block_id: BlockId) -> Result<Option<Block<types::H256>>, Error> {
        self.web3.eth().block(block_id).await.map_err(Error::Web3)
    }

    /// Get balance of given account.
    pub async fn balance(&self, addr: Address) -> Result<types::U256, Error> {
        self.web3
            .eth()
            .balance(addr, None)
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
        Ok(fut.await?)
    }

    pub async fn jobs_set_counter(&self, new: u128) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call_with_confirmations("set_counter", new, addr, Default::default(), 0);
        Ok(fut.await?)
    }

    pub async fn jobs_inc_counter(&self) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.call_with_confirmations("inc_counter", (), addr, Default::default(), 0);
        Ok(fut.await?)
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
        events: &[ethabi::Event],
    ) -> Result<impl Stream<Item = Result<JobsEvent, Error>>, Error> {
        let jobs = self.jobs()?;
        let filter = FilterBuilder::default().address(vec![jobs.address()]);

        let filter = filter.topics(
            Some(events.iter().map(|e| e.signature()).collect()),
            None,
            None,
            None,
        );

        let eth_subscribe = self.web3.eth_subscribe();
        let stream_fut = eth_subscribe.subscribe_logs(filter.build());
        let stream = stream_fut.await?;

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
    fn jobs_event(&self, event: JobsEventKind) -> Result<ethabi::Event, Error> {
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
        Ok(fut.await?)
    }

    ///  Send money to local address's pending account.
    // TODO: not possible from someone else, yet.
    pub async fn jobs_send_pending_money_local(
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
            Ok(fut.await?)
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
        Ok(fut.await?)
        // TODO: check new values
    }

    /// Get next nonce used when a new draft job will be created.
    /// This means thas every nonce strictly inferior to it may refer to an existing job.
    pub async fn jobs_get_next_nonce(&self) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("get_next_nonce", (), *addr, Default::default(), None);
        Ok(fut.await?)
    }

    /// Creates a new draft job based on given job, and return it's [`JobId`].
    /// Every data from the given job is set in the blockchain.
    ///
    /// The job won't be executed at once, there still needs to be enough money in
    /// the local address's pending account (e.g. using [`Chain::jobs_send_pending_money_local`]),
    /// and mark it as ready using [`Chain::jobs_lock`].
    // TODO: check new values
    // TODO: split in sub-functions ?
    pub async fn jobs_create_draft(&self, job: &Job) -> Result<u128, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let stream = self
            .jobs_subscribe_to_event_kind(JobsEventKind::JobNew)
            .await?;

        let fut = jobs.call_with_confirmations("create_draft", (), *addr, Default::default(), 0);
        fut.await?;

        // TODO: concurrency issues ?
        // TODO: We have to suppose the user hasn't created another job at the same time.
        // TODO: or use [`get_next_nonce`]?
        // TODO: check jobs values ?
        let nonce = stream
            .filter_map(|e| {
                future::ready(match e {
                    Ok(JobsEvent::JobNew { sender, nonce }) if sender == *addr => Some(nonce),
                    _ => None,
                })
            })
            .next()
            .await
            .ok_or(Error::CouldntFindJobNewEvent)?;
        let job_id_32 = JobId::job_id(addr, nonce).to_bytes32();

        let fut = jobs.call_with_confirmations(
            "set_parameters",
            (
                job_id_32,
                job.timeout(),
                job.redundancy(),
                job.max_failures(),
            ),
            *addr,
            Default::default(),
            0,
        );
        fut.await?;

        let other_data = job.other_data();
        let mut encoded_data = Vec::with_capacity(other_data.encoded_len());
        other_data.encode(&mut encoded_data)?;
        let fut = jobs.call_with_confirmations(
            "set_other_data",
            (job_id_32, encoded_data),
            *addr,
            Default::default(),
            0,
        );
        fut.await?;

        for arg in job.arguments().iter() {
            let fut = jobs.call_with_confirmations(
                "push_argument",
                (job_id_32, arg.clone()),
                *addr,
                Default::default(),
                0,
            );
            fut.await?;
        }

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
        fut.await?;

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
        fut.await?;

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
        if sender != *addr {
            return Err(Error::JobNotOurs(job_id.clone()));
        }
        if !self.jobs_is_draft(job_id, false).await? {
            return Err(Error::JobNotADraft(job_id.clone()));
        }

        let fut = jobs.call_with_confirmations(
            "delete_draft",
            job_id.to_bytes32(),
            *addr,
            Default::default(),
            0,
        );
        Ok(fut.await?)
    }

    /// Locks a job from draft to pending so it can be computed.
    /// The maximum amount of money it can need will be locked as well as all
    /// its parameters.
    pub async fn jobs_lock(&self, job_id: &JobId) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let job = self.jobs_get_job(job_id, true).await?;
        if job.sender() != *addr {
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
            jobs.call_with_confirmations("lock", job_id.to_bytes32(), *addr, Default::default(), 0);
        Ok(fut.await?)
    }

    /// Get [`Job::timeout`], [`Job::redundancy`] and [`Job::max_failures`] for given job.
    pub async fn jobs_get_parameters(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<(u64, u64, u64), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "get_parameters",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
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

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "get_other_data",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            let other_data_bytes: Vec<u8> = fut.await?;
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

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "get_arguments",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
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

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "get_worker_parameters",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
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

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "get_management_parameters",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
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

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "get_sender_nonce",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Check if a job id corresponds to an existing job.
    pub async fn jobs_is_job_non_null(&self, job_id: &JobId) -> Result<bool, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "is_job_non_null",
            job_id.to_bytes32(),
            *addr,
            Default::default(),
            None,
        );
        Ok(fut.await?)
    }

    /// Check if the job is a draft and can still be modified by its sender.
    pub async fn jobs_is_draft(&self, job_id: &JobId, check_non_null: bool) -> Result<bool, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "is_draft",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Check if all tasks are either completed or definitely failed.
    pub async fn jobs_is_completed(
        &self,
        job_id: &JobId,
        check_non_null: bool,
    ) -> Result<bool, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_job_non_null(job_id).await? {
            let fut = jobs.query(
                "is_job_completed",
                job_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
        } else {
            Err(Error::JobNotFound(job_id.clone()))
        }
    }

    /// Get a job from the blockchain.
    pub async fn jobs_get_job(&self, job_id: &JobId, check_non_null: bool) -> Result<Job, Error> {
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
            Multihash::from_bytes(&other_data.program_hash[..])?,
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

    /// Check if a task id corresponds to an existing task.
    pub async fn jobs_is_task_non_null(&self, task_id: &TaskId) -> Result<bool, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query(
            "is_task_non_null",
            task_id.to_bytes32(),
            *addr,
            Default::default(),
            None,
        );
        Ok(fut.await?)
    }

    /// Get the argument of a given task.
    pub async fn jobs_get_argument(
        &self,
        task_id: &TaskId,
        check_non_null: bool,
    ) -> Result<Vec<u8>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_task_non_null(task_id).await? {
            let fut = jobs.query(
                "get_argument",
                task_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            Ok(fut.await?)
        } else {
            Err(Error::TaskNotFound(task_id.clone()))
        }
    }

    /// Get the state of the given task.
    pub async fn jobs_get_task_state(
        &self,
        task_id: &TaskId,
        check_non_null: bool,
    ) -> Result<JobsCompleteness, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_task_non_null(task_id).await? {
            let fut = jobs.query(
                "get_task_state",
                task_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            let (state, reason, result) = fut.await?;
            let reason =
                try_convert_task_error_kind(reason).ok_or(Error::TaskErrorKindParse(reason))?;
            try_convert_tasks_state(state, result, reason).ok_or(Error::TaskStateParse(state))
        } else {
            Err(Error::TaskNotFound(task_id.clone()))
        }
    }

    /// Get the task's [`JobId`] and `argument_id`.
    /// Calculating the [`TaskId`] with them should the exact same value as the parameter
    /// `task_id`.
    pub async fn jobs_get_task(
        &self,
        task_id: &TaskId,
        check_non_null: bool,
    ) -> Result<(JobId, u128), Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        if !check_non_null || self.jobs_is_task_non_null(task_id).await? {
            let fut = jobs.query(
                "get_task",
                task_id.to_bytes32(),
                *addr,
                Default::default(),
                None,
            );
            let (job_id, argument_id): (Vec<u8>, u128) = fut.await?;
            Ok((JobId::try_from(&job_id[..])?, argument_id))
        } else {
            Err(Error::TaskNotFound(task_id.clone()))
        }
    }

    /// Get all the information related to given task.
    pub async fn jobs_get_full_task(
        &self,
        task_id: &TaskId,
        check_non_null: bool,
    ) -> Result<(JobId, u128, Vec<u8>, JobsCompleteness), Error> {
        if !check_non_null || self.jobs_is_task_non_null(task_id).await? {
            let (job_id, argument_id) = self.jobs_get_task(task_id, false).await?;
            let argument = self.jobs_get_argument(task_id, false).await?;
            let state = self.jobs_get_task_state(task_id, false).await?;
            Ok((job_id, argument_id, argument, state))
        } else {
            Err(Error::TaskNotFound(task_id.clone()))
        }
    }

    /// Gets the address of the oracle which can modify pending tasks.
    pub async fn jobs_oracle(&self) -> Result<Address, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;

        let fut = jobs.query("oracle", (), *addr, Default::default(), None);
        Ok(fut.await?)
    }

    /// Register managers who participated on the given task.
    ///
    /// > **Note:** Local address must be the oracle of the contract.
    pub async fn jobs_set_managers(
        &self,
        task_id: &TaskId,
        managers: &[Address],
    ) -> Result<Vec<types::TransactionReceipt>, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;
        let oracle = self.jobs_oracle().await?;

        if oracle == *addr && self.jobs_is_task_non_null(task_id).await? {
            let mut res = Vec::new();
            for manager in managers.iter() {
                let fut = jobs.call_with_confirmations(
                    "push_manager",
                    (task_id.to_bytes32(), *manager),
                    *addr,
                    Default::default(),
                    0,
                );
                res.push(fut.await?);
            }
            Ok(res)
        } else {
            Err(Error::LocalAddressNotOracle(*addr, oracle))
        }
    }

    /// Register a task is definitely failed and pay the managers and refund
    /// the sender with the remaining amount.
    ///
    /// > **Note:** Local address must be the oracle of the contract.
    pub async fn jobs_set_definitely_failed(
        &self,
        task_id: &TaskId,
        reason: TaskDefiniteErrorKind,
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;
        let oracle = self.jobs_oracle().await?;

        // TODO: check incomplete
        if oracle == *addr && self.jobs_is_task_non_null(task_id).await? {
            let reason = convert_task_error_kind(reason)
                .ok_or(Error::TaskErrorKindNotCompatibleWithJobs(reason))?;
            let fut = jobs.call_with_confirmations(
                "set_definitely_failed",
                (task_id.to_bytes32(), reason),
                *addr,
                Default::default(),
                0,
            );
            Ok(fut.await?)
        } else {
            Err(Error::LocalAddressNotOracle(*addr, oracle))
        }
    }

    /// Register a task is definitely failed and pay the managers and workers,
    /// and refund the sender with the remaining amount.
    ///
    /// > **Note:** Local address must be the oracle of the contract.
    pub async fn jobs_set_completed(
        &self,
        task_id: &TaskId,
        result: &[u8],
        workers_infos: &[WorkerPaymentInfo],
    ) -> Result<types::TransactionReceipt, Error> {
        let jobs = self.jobs()?;
        let addr = self.local_address()?;
        let oracle = self.jobs_oracle().await?;

        // TODO: check incomplete
        // TODO: check redundancy
        if oracle == *addr && self.jobs_is_task_non_null(task_id).await? {
            let mut worker_addrs = Vec::with_capacity(workers_infos.len());
            let mut worker_prices = Vec::with_capacity(workers_infos.len());
            let mut network_prices = Vec::with_capacity(workers_infos.len());

            workers_infos.iter().for_each(|w| {
                worker_addrs.push(*w.worker_address());
                worker_prices.push(w.worker_price());
                network_prices.push(w.network_price());
            });

            let fut = jobs.call_with_confirmations(
                "set_completed",
                (
                    task_id.to_bytes32(),
                    Vec::from(result),
                    worker_addrs,
                    worker_prices,
                    network_prices,
                ),
                *addr,
                Default::default(),
                0,
            );
            Ok(fut.await?)
        } else {
            Err(Error::LocalAddressNotOracle(*addr, oracle))
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
