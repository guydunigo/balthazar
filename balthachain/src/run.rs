use super::{
    config::{Address, ChainConfig},
    Chain, Error, JobsEventKind,
};
use futures::{executor::block_on, future, StreamExt};
use misc::{
    job::{BestMethod, Job, JobId, ProgramKind, TaskId},
    multihash::Multihash,
};
use web3::types::{BlockId, BlockNumber};

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
    /*
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
    // TODO...
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
        /*
        RunMode::JobsGetPendingJobs => {
            let jobs: Vec<String> = chain
                .jobs_get_pending_jobs()
                .await?
                .iter()
                .map(|h| format!("{}", h))
                .collect();
            println!("{:?}", jobs);
        }
        */
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
        /*
        RunMode::JobsSetResult {
            task_id,
            result,
            workers,
        } => {
            chain.jobs_set_result(task_id, result, &workers[..]).await?;
            println!("Result of task `{}` stored.", task_id,);
        }
        */
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
        } /*
          RunMode::JobsValidate { job_id } => {
              let job = chain.jobs_get_job(job_id).await?;
              println!("{}", job);
              chain.jobs_validate_results(job_id).await?;
              println!("{} validated, workers paid, and money available.", job_id);
          }
          */
    }

    Ok(())
}
