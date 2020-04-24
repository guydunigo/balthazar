use super::{config::ChainConfig, Chain, Error, JobsEvent, JobsEventKind};
use futures::{executor::block_on, StreamExt};
use misc::{
    job::{Job, JobId, ProgramKind, TaskId},
    multihash::Multihash,
    shared_state::WorkerPaymentInfo,
};
use proto::manager::TaskDefiniteErrorKind;
use web3::types::{Address, BlockId, BlockNumber};

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
    JobsSubscribe(Vec<JobsEventKind>, Option<usize>),
    /// List all Jobs smart contract events.
    JobsEvents,
    /// Create draft job and return the cost to pay.
    JobsCreateDraft {
        program_kind: ProgramKind,
        addresses: Vec<String>,
        program_hash: Multihash,
        arguments: Vec<Vec<u8>>,
        timeout: u64,
        // max_worker_price: u64,
        max_network_usage: u64,
        // max_network_price: u64,
        // min_checking_interval: u64,
        // management_price: u64,
        redundancy: u64,
        max_failures: u64,
        // best_method: BestMethod,
        // min_cpu_count: u64,
        // min_memory: u64,
        // min_network_speed: u64,
        is_program_pure: bool,
        lock: bool,
    },
    /// Remove a draft job.
    JobsDeleteDraft {
        job_id: JobId,
    },
    /// Lock a draft job if it meets readiness criteria and there's enough pending
    /// money, will lock the job and send the tasks for execution.
    JobsLock {
        job_id: JobId,
    },
    /// Get an already validated job.
    /// As well as its tasks statuses.
    JobsGetJob {
        job_id: JobId,
        /// Check if job is non_null before getting it.
        check_non_null: bool,
    },
    /// Get the list of pending drafts jobs as job ids.
    JobsGetDraftJobs,
    /// Get the list of jobs waiting for computation as job ids.
    JobsGetPendingJobs,
    /// Get the list of completed jobs as job ids.
    JobsGetCompletedJobs,
    /// Query a task.
    JobsGetTask {
        task_id: TaskId,
        /// Check if job is non_null before getting it.
        check_non_null: bool,
    },
    /// Send a result and workers to the sc.
    /// There must be as many workers as job's `redundancy`.
    JobsSetDefinitelyFailed {
        task_id: TaskId,
        reason: TaskDefiniteErrorKind,
        managers: Vec<Address>,
    },
    /// Send a result and workers to the sc.
    /// There must be as many workers as job's `redundancy`.
    JobsSetCompleted {
        task_id: TaskId,
        result: Vec<u8>,
        managers: Vec<Address>,
        /// Workers ethereum address, worker_price, network_price.
        workers: Vec<(Address, u64, u64)>,
    },
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
        /*
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
        */
        RunMode::JobsSubscribe(events, nb) => {
            async fn for_each(e: Result<JobsEvent, Error>) {
                match e {
                    Ok(evt) => println!("{}", evt),
                    Err(_) => println!("{:?}", e),
                }
            }

            let nb = if let Some(nb) = nb {
                *nb
            } else {
                // TODO: not very clean but shouldn't be hit anytime soon...
                usize::MAX
            };

            if events.is_empty() {
                chain
                    .jobs_subscribe()
                    .await?
                    .take(nb)
                    .for_each(for_each)
                    .await;
            } else {
                chain
                    .jobs_subscribe_to_event_kinds(&events[..])
                    .await?
                    .take(nb)
                    .for_each(for_each)
                    .await;
            };
        }
        RunMode::JobsEvents => {
            for evt in chain.jobs_events()? {
                println!("{:?}, {}", evt, evt.signature());
            }
        }
        RunMode::JobsCreateDraft {
            program_kind,
            addresses,
            program_hash,
            arguments,
            timeout,
            // max_worker_price,
            max_network_usage,
            // max_network_price,
            // min_checking_interval,
            // management_price,
            redundancy,
            max_failures,
            // best_method,
            // min_cpu_count,
            // min_memory,
            // min_network_speed,
            is_program_pure,
            lock,
        } => {
            let mut job = Job::new(
                *program_kind,
                addresses.clone(),
                program_hash.clone(),
                arguments.clone(),
                *chain.local_address()?,
            );
            job.set_timeout(*timeout);
            // job.set_max_worker_price(*max_worker_price);
            job.set_max_network_usage(*max_network_usage);
            // job.set_max_network_price(*max_network_price);
            // job.set_min_checking_interval(*min_checking_interval);
            // job.set_management_price(*management_price);
            job.set_redundancy(*redundancy);
            job.set_max_failures(*max_failures);
            // job.set_best_method(*best_method);
            // job.set_min_cpu_count(*min_cpu_count);
            // job.set_min_memory(*min_memory);
            // job.set_min_network_speed(*min_network_speed);
            job.set_is_program_pure(*is_program_pure);

            let mut job = job.clone();
            let nonce = chain.jobs_create_draft(&job).await?;
            job.set_nonce(Some(nonce));
            let job_id = job.job_id().expect("nonce just set");

            println!("{}", chain.jobs_get_job(&job_id, true).await?);
            println!("Draft stored as {} !", job_id);

            if !lock {
                println!("You might still need to send the money for it and set it as locked.");
            } else {
                chain
                    .jobs_send_pending_money_local(job.calc_max_price().into())
                    .await?;
                chain.jobs_lock(&job_id).await?;
                println!("{} set as pending for work.", job_id);
            }
        }
        RunMode::JobsDeleteDraft { job_id } => {
            let job = chain.jobs_get_job(job_id, true).await?;
            println!("{}", job);
            chain.jobs_delete_draft(job_id).await?;
            println!("{} deleted!", job_id);
        }
        RunMode::JobsLock { job_id } => {
            let job = chain.jobs_get_job(job_id, true).await?;
            println!("{}", job);
            chain.jobs_lock(job_id).await?;
            println!("{} set as pending for work.", job_id);
        }
        RunMode::JobsGetJob {
            job_id,
            check_non_null,
        } => {
            let job = chain.jobs_get_job(job_id, *check_non_null).await?;
            println!("{}", job);
            print!("State: ");
            if chain.jobs_is_draft(job_id, *check_non_null).await? {
                println!("Draft");
            } else if chain.jobs_is_completed(job_id, *check_non_null).await? {
                println!("Completed");
            } else {
                println!("Pending");
            }
        }
        RunMode::JobsGetDraftJobs => {
            let jobs = chain.jobs_get_draft_jobs_local().await?;
            println!("{:?}", jobs);
        }
        RunMode::JobsGetPendingJobs => {
            let jobs = chain.jobs_get_pending_jobs_local().await?;
            println!("{:?}", jobs);
        }
        RunMode::JobsGetCompletedJobs => {
            let jobs = chain.jobs_get_completed_jobs_local().await?;
            println!("{:?}", jobs);
        }
        RunMode::JobsGetTask {
            task_id,
            check_non_null,
        } => {
            let task = chain.jobs_get_full_task(task_id, *check_non_null).await?;
            // TODO: better printing.
            println!("{:?}", task);
        }
        RunMode::JobsSetDefinitelyFailed {
            task_id,
            reason,
            managers,
        } => {
            chain.jobs_set_managers(task_id, &managers[..]).await?;
            chain.jobs_set_definitely_failed(task_id, *reason).await?;
            println!("Result of task `{}` stored.", task_id,);
        }
        RunMode::JobsSetCompleted {
            task_id,
            result,
            managers,
            workers,
        } => {
            let workers: Vec<WorkerPaymentInfo> = workers
                .iter()
                .map(|(w, p, n)| WorkerPaymentInfo::new(*w, *p, *n))
                .collect();
            chain.jobs_set_managers(task_id, &managers[..]).await?;
            chain
                .jobs_set_completed(task_id, result, &workers[..])
                .await?;
            println!("Result of task `{}` stored.", task_id,);
        }
        RunMode::JobsGetMoney => {
            let (pending, locked) = chain.jobs_get_pending_locked_money_local().await?;
            println!(
                "Pending money: {} money.\nLocked money: {} money.",
                pending, locked
            );
        }
        RunMode::JobsSendMoney { amount } => {
            chain
                .jobs_send_pending_money_local((*amount).into())
                .await?;
            println!("{} money sent.", amount);
        }
        RunMode::JobsRecoverMoney { amount } => {
            chain.jobs_recover_pending_money((*amount).into()).await?;
            println!("{} money recovered.", amount);
        }
    }

    Ok(())
}
