extern crate balthachain;
extern crate balthamisc;
extern crate futures;

use futures::executor::block_on;

use balthachain::{Chain, ChainConfig, Error};
use balthamisc::{
    job::{Job, JobId, ProgramKind},
    multiformats::try_decode_multibase_multihash_string,
};
use std::fs::{read, read_to_string};

const JOBS_ABI_PATH: &str = "./contracts/Jobs.json";
const JOBS_ADDRESS_PATH: &str = "./contracts/jobs_address";
const ACCOUNT_ADDRESS_PATH: &str = "./chain/parity_account";

fn config() -> ChainConfig {
    let mut conf = ChainConfig::default();

    {
        let addr = read_to_string(ACCOUNT_ADDRESS_PATH).unwrap_or_else(|e| {
            panic!(
                "Could not open ethereum address file {}: {}",
                ACCOUNT_ADDRESS_PATH, e
            )
        });
        let addr = addr.parse().unwrap_or_else(|e| {
            panic!(
                "Could not parse address from file {}: {}",
                ACCOUNT_ADDRESS_PATH, e
            )
        });
        conf.set_ethereum_address(Some(addr));
    }
    {
        let addr = read_to_string(JOBS_ADDRESS_PATH).unwrap_or_else(|e| {
            panic!(
                "Could not open jobs sc address file {}: {}",
                JOBS_ADDRESS_PATH, e
            )
        });
        let addr = addr.parse().unwrap_or_else(|e| {
            panic!(
                "Could not parse jobs address from file {}: {}",
                JOBS_ADDRESS_PATH, e
            )
        });
        let abi = read(JOBS_ABI_PATH)
            .unwrap_or_else(|e| panic!("Could not open jobs sc abi file {}: {}", JOBS_ABI_PATH, e));

        conf.set_contract_jobs(Some((addr, abi)));
    }

    conf
}

// TODO: check events

#[test]
fn it_can_process_a_new_job() -> Result<(), Error> {
    let conf = config();
    let chain = Chain::new(&conf);

    let mut job = Job::new(
        ProgramKind::Wasm0m1n0,
        vec!["/ipfs/QmZbABTQy1dHPrimGNhUeZKRnesiJ2RnMNJgDtc65KgnJv".to_string()],
        try_decode_multibase_multihash_string("MGyC7DRF34gDhPAY8HzBm9qAIceHwy7n0pCItA1teasyEyg==")
            .unwrap(),
        vec![b"1234".to_vec(), b"12345".to_vec(), b"404".to_vec()],
        conf.ethereum_address().unwrap(),
    );
    job.set_timeout(10);
    job.set_max_worker_price(10);
    job.set_max_network_usage(10);
    job.set_max_network_price(10);
    job.set_min_checking_interval(15);
    job.set_management_price(10);
    job.set_redundancy(5);
    job.set_max_failures(5);
    job.set_is_program_pure(true);

    let nonce = block_on(chain.jobs_create_draft(&job))?;
    job.set_nonce(Some(nonce));
    let job_id = job.job_id().expect("we just set it");
    let job_2 = block_on(chain.jobs_get_job(&job_id, true))?;
    assert_eq!(job, job_2, "Sent job and draft are different!");

    {
        let nonce = block_on(chain.jobs_create_draft(&job))?;
        let job_id = JobId::job_id(&job.sender(), nonce);
        assert!(
            block_on(chain.jobs_is_job_non_null(&job_id))?,
            "Job is still null and hasn't been created."
        );
        block_on(chain.jobs_delete_draft(&job_id))?;
        assert!(
            !block_on(chain.jobs_is_job_non_null(&job_id))?,
            "Job isn't still null and hasn't been deleted."
        );
    }

    let (pending_0, _) = block_on(chain.jobs_get_pending_locked_money_local())?;
    block_on(chain.jobs_send_pending_money_local(job.calc_max_price().into()))?;
    let (pending_1, locked_0) = block_on(chain.jobs_get_pending_locked_money_local())?;
    assert_eq!(
        pending_1,
        pending_0 + job.calc_max_price(),
        "Invalid pending money after transfer!"
    );

    block_on(chain.jobs_lock(&job_id))?;
    let (pending_2, locked_1) = block_on(chain.jobs_get_pending_locked_money_local())?;
    assert_eq!(
        pending_2,
        pending_1 - job.calc_max_price(),
        "Invalid pending money after ready!"
    );
    assert_eq!(
        locked_1,
        locked_0 + job.calc_max_price(),
        "Invalid locked money after ready!"
    );

    // TODO: check tasks
    // TODO: set one completed
    // TODO: check task state
    // TODO: check money
    // TODO: set one def failed
    // TODO: check task state
    // TODO: check money
    // TODO: set the third
    // TODO: check task state
    // TODO: check money
    // TODO: check job complete
    //
    // TODO: check ALL events

    Ok(())
}
