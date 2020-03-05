//! Look at [the `clap` documentation](`clap`)
//! and [the `structopt` documentation](https://docs.rs/structopt/0.3.9/structopt/)
#![allow(clippy::redundant_clone)]
extern crate multiaddr;

use clap::{arg_enum, Clap};
// TODO: use uniform Multiaddr
use lib::{
    chain::{self, Address, JobsEventKind},
    misc::{
        job::{JobId, ProgramKind, TaskId},
        multiformats::{self as formats, try_decode_multibase_multihash_string},
        multihash::Multihash,
    },
    net::Multiaddr as Libp2pMultiaddr,
    proto::{NodeType, NodeTypeContainer},
    store::ipfs::IpfsStorageCreationError,
    store::{Multiaddr, StorageType},
    BalthazarConfig, RunMode,
};
use std::{
    convert::TryInto,
    fs::read,
    io,
    io::{stdin, Read},
    path::PathBuf,
    str::FromStr,
};

#[derive(Debug)]
pub enum ParseArgsError {
    IpfsError(IpfsStorageCreationError),
    ContractJobsAbiFileReadError(io::Error),
    WasmProgramFileReadError(io::Error),
    MiscError(io::Error),
}

impl From<IpfsStorageCreationError> for ParseArgsError {
    fn from(e: IpfsStorageCreationError) -> Self {
        ParseArgsError::IpfsError(e)
    }
}

#[derive(Clap, Clone)]
#[clap(rename_all = "kebab-case")]
pub enum Subcommand {
    /// Starts as a worker node.
    Worker {
        /// Multiaddresses of a managers this worker is authorized to respond to.
        /// Those multiaddresses will be dialed the same way as the `--peer` command.
        /// If not defined, will accept any manager.
        /// (e.g. `/ip4/10.0.0.1/tcp/5003`, `/p2p/[PEER_ID]`, `/ip4/10.0.0.1/tcp/5003/p2p/[PEER_ID]`,
        /// ...)
        #[clap(short, long, requires("worker"), number_of_values(1))]
        authorized_managers: Vec<Libp2pMultiaddr>,
    },
    /// Starts as a manager node.
    Manager {
        /// Provide a wasm program that will be passed to workers.
        #[clap(name = "wasm", short, long, requires("args"))]
        wasm_file_addr: Option<String>,
        /// Arguments to pass to the program.
        #[clap(short, long, number_of_values(1))]
        args: Option<Vec<String>>,
    },
    /// Interract with the blockchain.
    Chain(ChainSub),
    /// Interract with the storages directly.
    Storage,
    /// Run wasm programs.
    Runner {
        /// Provide a wasm program that will be passed to workers.
        wasm_file_path: PathBuf,
        /// Arguments to pass to the program.
        args: String,
        /// Number of times to run the program.
        nb_times: Option<usize>,
    },
    /// Run the balthwasm program natively.
    Native {
        /// Arguments to pass to the program.
        args: String,
        /// Number of times to run the program.
        nb_times: Option<usize>,
    },
    /// Miscelanous tools
    Misc(MiscSub),
}

impl std::convert::TryInto<RunMode> for Subcommand {
    type Error = ParseArgsError;

    fn try_into(self) -> Result<RunMode, ParseArgsError> {
        let res = match self {
            Subcommand::Worker { .. } | Subcommand::Manager { .. } => RunMode::Node,
            Subcommand::Chain(mode) => RunMode::Blockchain(mode.into()),
            Subcommand::Storage => RunMode::Storage,
            Subcommand::Runner {
                wasm_file_path,
                args,
                nb_times,
            } => RunMode::Runner(
                read(wasm_file_path).map_err(ParseArgsError::WasmProgramFileReadError)?,
                args.clone().into_bytes(),
                nb_times.unwrap_or(1),
            ),
            Subcommand::Native { args, nb_times } => {
                RunMode::Native(args.clone().into_bytes(), nb_times.unwrap_or(1))
            }
            // TODO: not clone, use references
            Subcommand::Misc(mode) => {
                RunMode::Misc(mode.try_into().map_err(ParseArgsError::MiscError)?)
            }
        };

        Ok(res)
    }
}

#[derive(Clap, Clone)]
#[clap(rename_all = "kebab-case")]
pub enum MiscSub {
    /// Hash given data of file.
    /// If no flag is provided, reads from stdin.
    Hash {
        /// Hash given string.
        #[clap(short, long, conflicts_with("file"))]
        data: Option<String>,
        /// Hash file.
        #[clap(short, long)]
        file: Option<String>,
    },
    /// Check given hash and data or file.
    Check {
        /// Multibase encoded multihash of the data.
        #[clap(parse(try_from_str = try_decode_multibase_multihash_string))]
        hash: Multihash,
        /// Check given string.
        #[clap(short, long, conflicts_with("file"))]
        data: Option<String>,
        /// Check given file.
        #[clap(short, long)]
        file: Option<String>,
    },
}

impl std::convert::TryInto<formats::RunMode> for MiscSub {
    type Error = io::Error;

    fn try_into(self) -> Result<formats::RunMode, Self::Error> {
        fn get_data(data: Option<String>, file: Option<String>) -> Result<Vec<u8>, io::Error> {
            let data = match (file, data) {
                (Some(file), _) => read(file)?,
                (None, Some(data)) => data.into_bytes(),
                (None, None) => {
                    let mut vec = Vec::new();
                    stdin().read_to_end(&mut vec)?;
                    vec
                }
            };
            Ok(data)
        }

        let res = match self {
            MiscSub::Hash { data, file } => formats::RunMode::Hash(get_data(data, file)?),
            MiscSub::Check { hash, data, file } => {
                formats::RunMode::Check(hash, get_data(data, file)?)
            }
        };

        Ok(res)
    }
}

#[derive(Clap, Clone)]
#[clap(rename_all = "kebab-case")]
pub enum ChainSub {
    /// Get latest block information.
    Block,
    /// Get balance of provided address or local address.
    Balance {
        /// Address of account, if none is provided, will use `addr`.
        address: Option<Address>,
    },
    /// Actions related to Jobs smart-contract.
    Jobs(ChainJobsSub),
}

#[derive(Clap, Clone)]
#[clap(rename_all = "kebab-case")]
pub enum ChainJobsSub {
    /// Get value of `counter` or modify it.
    Counter {
        /// Increment counter value.
        #[clap(short, long)]
        inc: bool,
        /// Set the counter to given value.
        #[clap(short, long, conflicts_with("inc"))]
        set: Option<u128>,
    },
    /// Subscribe to contract's events.
    Subscribe { events_names: Vec<JobsEventKind> },
    /// List events and their parameters.
    Events,
    /// Create a new draft.
    Draft {
        #[clap(parse(try_from_str = try_decode_multibase_multihash_string))]
        program_hash: Multihash,
        #[clap(short, long, number_of_values(1))]
        addresses: Vec<Multiaddr>,
        #[clap(name = "input", short, long, number_of_values(1))]
        arguments: Vec<String>,
        timeout: u64,
        max_failures: u64,
        best_method: u64,
        max_worker_price: u64,
        min_cpu_count: u64,
        min_memory: u64,
        max_network_usage: u64,
        max_network_price: u64,
        min_network_speed: u64,
        redundancy: u64,
        /// Includes tests.
        #[clap(name = "pure", short, long)]
        is_program_pure: bool,
    },
    /// Display a job and all its information.
    Get {
        /// Provide the job id for a pending job.
        // TODO: at least of of both required
        #[clap(name = "pending", short, long, conflicts_with("draft"))]
        job_id: Option<JobId>,
        /// Provide the nonce of a draft job.
        #[clap(name = "draft", short, long)]
        nonce: Option<u128>,
    },
    /// Display all pending jobs or drafts.
    GetAll {
        /// Provide the nonce of a draft job instead of pending ones.
        #[clap(short, long)]
        drafts: bool,
    },
    /// Delete a draft.
    Delete { nonce: u128 },
    /// Get result for given task or set it if `--set` is provided.
    Result {
        task_id: TaskId,
        /// Set result instead of getting it.
        #[clap(short, long, requires("workers"))]
        set: Option<String>,
        #[clap(name = "workers", short, long, number_of_values(1), parse(try_from_str = try_parse_workers))]
        workers: Option<Vec<(Address, u64, u64)>>,
    },
    /// Get or send money to the Jobs smart-contract.
    Money {
        /// Send the given value to the pending account
        #[clap(short, long, conflicts_with("recover"))]
        send: Option<u128>,
        /// Get money from the Jobs smart-contract.
        #[clap(short, long)]
        recover: Option<u128>,
    },
    /// Set a job as ready, if it meets readiness criteria and there's enough pending
    /// money, will lock the job and send the tasks for execution.
    Ready { nonce: u128 },
    /// When all the tasks have a corresponding result, the owner can set job as validated,
    /// to pay the workers and unlock the remaning money.
    Validate { job_id: JobId },
}

impl Into<chain::RunMode> for ChainSub {
    fn into(self) -> chain::RunMode {
        match self {
            ChainSub::Block => chain::RunMode::Block,
            ChainSub::Balance { address } => chain::RunMode::Balance(address),
            ChainSub::Jobs(ChainJobsSub::Counter {
                set: None,
                inc: false,
            }) => chain::RunMode::JobsCounterGet,
            ChainSub::Jobs(ChainJobsSub::Counter { set: Some(new), .. }) => {
                chain::RunMode::JobsCounterSet(new)
            }
            ChainSub::Jobs(ChainJobsSub::Counter {
                set: None,
                inc: true,
            }) => chain::RunMode::JobsCounterInc,
            ChainSub::Jobs(ChainJobsSub::Subscribe { events_names }) => {
                chain::RunMode::JobsSubscribe(events_names)
            }
            ChainSub::Jobs(ChainJobsSub::Events) => chain::RunMode::JobsEvents,
            ChainSub::Jobs(ChainJobsSub::Draft {
                program_hash,
                addresses,
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
            }) => chain::RunMode::JobsNewDraft {
                program_kind: ProgramKind::Wasm,
                addresses,
                program_hash,
                arguments: arguments.iter().map(|s| s.clone().into_bytes()).collect(),
                timeout,
                max_failures,
                best_method: best_method.try_into().unwrap(),
                max_worker_price,
                min_cpu_count,
                min_memory,
                max_network_usage,
                max_network_price,
                min_network_speed,
                redundancy,
                is_program_pure,
            },
            ChainSub::Jobs(ChainJobsSub::Get {
                job_id: Some(job_id),
                ..
            }) => chain::RunMode::JobsGetJob { job_id },
            ChainSub::Jobs(ChainJobsSub::Get {
                nonce: Some(nonce), ..
            }) => chain::RunMode::JobsGetDraftJob { nonce },
            ChainSub::Jobs(ChainJobsSub::Get { .. }) => unreachable!(),
            ChainSub::Jobs(ChainJobsSub::GetAll { drafts: false }) => {
                chain::RunMode::JobsGetPendingJobs
            }
            ChainSub::Jobs(ChainJobsSub::GetAll { drafts: true }) => {
                chain::RunMode::JobsGetDraftJobs
            }
            ChainSub::Jobs(ChainJobsSub::Delete { nonce }) => {
                chain::RunMode::JobsDeleteDraftJob { nonce }
            }
            ChainSub::Jobs(ChainJobsSub::Result {
                task_id, set: None, ..
            }) => chain::RunMode::JobsGetResult { task_id },
            ChainSub::Jobs(ChainJobsSub::Result {
                task_id,
                set: Some(result),
                workers: Some(workers),
            }) => chain::RunMode::JobsSetResult {
                task_id,
                result: result.clone().into_bytes(),
                workers,
            },
            ChainSub::Jobs(ChainJobsSub::Result { .. }) => unreachable!(),
            ChainSub::Jobs(ChainJobsSub::Money {
                send: None,
                recover: None,
            }) => chain::RunMode::JobsGetMoney,
            ChainSub::Jobs(ChainJobsSub::Money {
                send: Some(amount),
                recover: None,
            }) => chain::RunMode::JobsSendMoney { amount },
            ChainSub::Jobs(ChainJobsSub::Money {
                recover: Some(amount),
                ..
            }) => chain::RunMode::JobsRecoverMoney { amount },
            ChainSub::Jobs(ChainJobsSub::Ready { nonce }) => chain::RunMode::JobsReady { nonce },
            ChainSub::Jobs(ChainJobsSub::Validate { job_id }) => {
                chain::RunMode::JobsValidate { job_id }
            }
        }
    }
}

// TODO: validators
#[derive(Clap, Clone)]
#[clap(rename_all = "kebab-case")]
pub struct BalthazarArgs {
    #[clap(subcommand)]
    subcommand: Subcommand,

    /// Disable locally listening, only dialing towards others peers.
    #[clap(short = "L", long)]
    disable_listen: bool,
    /// Set a address to listen on, default : `/ip4/0.0.0.0/tcp/5003`.
    // TODO: extract value from const
    #[clap(
        name = "listen",
        short,
        long,
        conflicts_with("disable_listen"),
        group("Network")
    )]
    listen_addr: Option<Libp2pMultiaddr>,
    /// Peer to connect to when started (e.g. `/ip4/0.0.0.0/tcp/5003`).
    #[clap(name = "peer", short, long, number_of_values(1))]
    peers: Vec<Libp2pMultiaddr>,

    /// Address to connect to a running IPFS daemon, default: address in file `~/.ipfs/api` or `/ip4/127.0.0.1/5001`.
    #[clap(short, long)]
    ipfs_api: Option<Multiaddr>,
    /// Storage to use as default for storing files, as well as for getting files when source
    /// couldn't be determined.
    #[clap(short = "s", long, case_insensitive(true),
        possible_values(&DefaultStorageArg::variants()),
        parse(try_from_str = try_parse_default_storage),
    )]
    default_storage: Option<StorageType>,

    /// The websocket address to connect the Ethereum json RPC endpoint.
    /// Default to `ws://localhost:8546`.
    #[clap(short, long)]
    web3_ws: Option<String>,
    /// Local ethereum address to use.
    #[clap(name = "addr", long)]
    ethereum_address: Option<Address>,
    /// Password to the account.
    #[clap(name = "pass", long, requires("addr"))]
    ethereum_password: Option<String>,
    /// Jobs contract address.
    #[clap(name = "jobs-address", long, requires("jobs-abi"))]
    contract_jobs_address: Option<Address>,
    /// Jobs contract path to json ABI file.
    #[clap(name = "jobs-abi", long)]
    contract_jobs_abi: Option<PathBuf>,
}

impl std::convert::TryInto<(RunMode, BalthazarConfig)> for BalthazarArgs {
    type Error = ParseArgsError;

    fn try_into(self) -> Result<(RunMode, BalthazarConfig), Self::Error> {
        let mut config = BalthazarConfig::default();

        let run_mode = self.subcommand.clone().try_into()?;

        match self.subcommand {
            Subcommand::Worker {
                authorized_managers,
            } => {
                config.set_node_type(NodeType::Worker);

                if let NodeTypeContainer::Worker(ref mut worker_mut) =
                    config.net_mut().node_type_configuration_mut()
                {
                    worker_mut
                        .authorized_managers_mut()
                        .extend_from_slice(&authorized_managers[..]);
                }

                // TODO: check the addresses before ?
                config
                    .net_mut()
                    .bootstrap_peers_mut()
                    .extend_from_slice(&authorized_managers[..]);
            }
            Subcommand::Manager {
                wasm_file_addr,
                args,
            } => {
                config.set_node_type(NodeType::Manager);

                if let (Some(wasm), Some(args)) = (wasm_file_addr, args) {
                    config.set_wasm(Some((
                        wasm.into_bytes(),
                        args.iter().map(|a| a.clone().into_bytes()).collect(),
                    )));
                }
            }
            _ => {}
        }

        {
            let net = config.net_mut();
            if self.disable_listen {
                net.set_listen_addr(None);
            } else if let Some(listen_addr) = self.listen_addr {
                net.set_listen_addr(Some(listen_addr));
            }

            net.bootstrap_peers_mut().extend_from_slice(&self.peers[..]);
        }
        {
            let store = config.storage_mut();
            if let Some(ipfs_api) = self.ipfs_api {
                store.set_ipfs_api(Some(ipfs_api))?;
            }
            if let Some(default_storage) = self.default_storage {
                store.set_default_storage(default_storage);
            }
        }
        {
            let chain = config.chain_mut();
            if let Some(web3_ws) = self.web3_ws {
                chain.set_web3_ws(web3_ws);
            }
            if let Some(ethereum_address) = self.ethereum_address {
                chain.set_ethereum_address(Some(ethereum_address));
            }
            if let Some(ethereum_password) = self.ethereum_password {
                chain.set_ethereum_password(Some(ethereum_password));
            }
            if let (Some(address), Some(abi_path)) =
                (self.contract_jobs_address, self.contract_jobs_abi)
            {
                let abi = read(abi_path).map_err(ParseArgsError::ContractJobsAbiFileReadError)?;
                chain.set_contract_jobs(Some((address, abi)));
            }
        }

        Ok((run_mode, config))
    }
}

/*
arg_enum! {
    #[derive(PartialEq, Debug)]
    pub enum NodeTypeArg {
        Worker,
        W,
        Manager,
        M,
    }
}

impl Into<NodeType> for NodeTypeArg {
    fn into(self) -> NodeType {
        match self {
            NodeTypeArg::Worker | NodeTypeArg::W => NodeType::Worker,
            NodeTypeArg::Manager | NodeTypeArg::M => NodeType::Manager,
        }
    }
}

fn try_parse_node_type(s: &str) -> Result<NodeType, String> {
    NodeTypeArg::from_str(s).map(|a| a.into())
}
*/

arg_enum! {
    #[derive(PartialEq, Debug)]
    pub enum DefaultStorageArg {
        Ipfs,
    }
}

impl Into<StorageType> for DefaultStorageArg {
    fn into(self) -> StorageType {
        match self {
            DefaultStorageArg::Ipfs => StorageType::Ipfs,
        }
    }
}

fn try_parse_default_storage(s: &str) -> Result<StorageType, String> {
    DefaultStorageArg::from_str(s).map(|a| a.into())
}

fn try_parse_workers(s: &str) -> Result<(Address, u64, u64), String> {
    let mut iter = s.split(",");
    let address = iter
        .next()
        .ok_or_else(|| "no worker".to_string())?
        .parse()
        .map_err(|_| "invalid worker address".to_string())?;
    let worker_price = iter
        .next()
        .ok_or_else(|| "no worker_price".to_string())?
        .parse()
        .map_err(|_| "invalid worker price".to_string())?;
    let network_price = iter
        .next()
        .ok_or_else(|| "no network_price".to_string())?
        .parse()
        .map_err(|_| "invalid network price".to_string())?;

    Ok((address, worker_price, network_price))
}
