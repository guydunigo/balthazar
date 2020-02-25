//! Look at [the `clap` documentation](https://github.com/clap-rs/clap)
//! and [the `stroctopt` documentation](https://docs.rs/structopt/0.3.9/structopt/)
#![allow(clippy::redundant_clone)]
extern crate multiaddr;

use clap::{arg_enum, Clap};
// TODO: use uniform Multiaddr
use lib::{
    misc::{NodeType, NodeTypeContainer},
    net::Multiaddr as Libp2pMultiaddr,
    store::{Multiaddr, StorageType},
    BalthazarConfig, RunMode,
};
use std::str::FromStr;

#[derive(Clap)]
#[clap(rename_all = "kebab-case")]
pub enum Subcommand {
    /// Starts as a worker node.
    Worker {
        /// Multiaddresses of a managers this worker is authorized to respond to.
        /// Those multiaddresses will be dialed the same way as the `--peer` command.
        /// If not defined, will accept any manager.
        /// (e.g. `/ip4/10.0.0.1/tcp/5003`, `/p2p/[PEER_ID]`, `/ip4/10.0.0.1/tcp/5003/p2p/[PEER_ID]`,
        /// ...)
        #[clap(short, long, requires("worker"))]
        authorized_managers: Option<Vec<Libp2pMultiaddr>>,
    },
    /// Starts as a manager node.
    Manager {
        /// Provide a wasm program that will be passed to workers.
        #[clap(long, conflicts_with("worker"), requires("args"))]
        wasm: Option<String>,
        /// Arguments to pass to the program.
        #[clap(long, conflicts_with("worker"))]
        args: Option<String>,
    },
    /// Interract with the blockchain.
    Blockchain,
    /// Interract with the storages directly.
    Storage,
    /// Run programs.
    Runner,
}

impl Into<RunMode> for &Subcommand {
    fn into(self) -> RunMode {
        match self {
            Subcommand::Worker { .. } | Subcommand::Manager { .. } => RunMode::Node,
            Subcommand::Blockchain => RunMode::Blockchain,
            Subcommand::Storage => RunMode::Storage,
            Subcommand::Runner => RunMode::Runner,
        }
    }
}

impl Into<RunMode> for Subcommand {
    fn into(self) -> RunMode {
        (&self).into()
    }
}

// TODO: validators

// TODO: use crate name directly ?
#[derive(Clap)]
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
        short = "l",
        long = "listen",
        conflicts_with("disable_listen"),
        group("Network")
    )]
    listen_addr: Option<Libp2pMultiaddr>,
    /// Peer to connect to when started (e.g. `/ip4/0.0.0.0/tcp/5003`).
    #[clap(name = "peer", short, long)]
    peers: Option<Vec<Libp2pMultiaddr>>,

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
}

impl std::convert::TryInto<(RunMode, BalthazarConfig)> for BalthazarArgs {
    type Error = lib::store::ipfs::IpfsStorageCreationError;

    fn try_into(self) -> Result<(RunMode, BalthazarConfig), Self::Error> {
        let mut config = BalthazarConfig::default();

        let run_mode = (&self.subcommand).into();

        match self.subcommand {
            Subcommand::Worker {
                authorized_managers,
            } => {
                config.set_node_type(NodeType::Worker);

                if let Some(authorized_managers) = authorized_managers {
                    if let NodeTypeContainer::Worker(ref mut worker_mut) =
                        config.net_mut().node_type_configuration_mut()
                    {
                        worker_mut
                            .authorized_managers_mut()
                            .extend_from_slice(&authorized_managers[..]);
                    } else {
                        panic!("`authorized_manager` was defined when node type isn't `Worker`.");
                    }

                    // TODO: check the addresses before ?
                    config
                        .net_mut()
                        .bootstrap_peers_mut()
                        .extend_from_slice(&authorized_managers[..]);
                }
            }
            Subcommand::Manager { wasm, args } => {
                config.set_node_type(NodeType::Manager);

                if let (Some(wasm), Some(args)) = (wasm, args) {
                    *config.wasm_mut() = Some((wasm.into_bytes(), args.into_bytes()));
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

            if let Some(peers) = self.peers {
                net.bootstrap_peers_mut().extend_from_slice(&peers[..]);
            }
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
