//! Look at [the `clap` documentation](https://github.com/clap-rs/clap)
//! and [the `stroctopt` documentation](https://docs.rs/structopt/0.3.9/structopt/)
#[allow(clippy::redundant_clone)]
extern crate multiaddr;

use clap::{arg_enum, Clap};
// TODO: use uniform Multiaddr
use lib::{
    misc::{NodeType, NodeTypeContainer},
    net::Multiaddr as Libp2pMultiaddr,
    store::{Multiaddr, StorageType},
    BalthazarConfig,
};
use std::str::FromStr;

// TODO: validators

// TODO: use crate name directly ?
#[derive(Clap)]
#[clap(rename_all = "kebab-case")]
pub struct BalthazarArgs {
    /*
    /// Node type, currently only Worker or Manager.
    #[clap(short = "t",
        long = "type",
        case_insensitive(true),
        possible_values(&NodeTypeArg::variants()),
        parse(try_from_str = try_parse_node_type),
    )]
    node_type: Option<NodeType>,
    */
    /// Starts as a manager (default).
    #[clap(name = "manager", short, long, conflicts_with("worker"))]
    _is_manager: bool,
    /// Starts as a worker.
    #[clap(name = "worker", short, long, conflicts_with("manager"))]
    is_worker: bool,
    /// Disable locally listening, only dialing towards others peers.
    #[clap(short = "L", long, group("Network"))]
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
    #[clap(name = "peer", short, long, group("Network"))]
    peers: Option<Vec<Libp2pMultiaddr>>,
    /// Address to connect to a running IPFS daemon, default: address in file `~/.ipfs/api` or `/ip4/127.0.0.1/5001`.
    #[clap(short, long, group("Storage"))]
    ipfs_api: Option<Multiaddr>,
    /// Storage to use as default for storing files, as well as for getting files when source
    /// couldn't be determined.
    #[clap(short = "s", long, case_insensitive(true),
        possible_values(&DefaultStorageArg::variants()),
        parse(try_from_str = try_parse_default_storage),
        group("Storage"),
    )]
    default_storage: Option<StorageType>,
    /// Multiaddress of a managers this worker is athourized to respond to.
    /// If not defined, will accept any manager.
    /// (e.g. `/ip4/10.0.0.1/tcp/5003`, `/p2p/[PEER_ID]`, `/ip4/10.0.0.1/tcp/5003/p2p/[PEER_ID]`,
    /// ...)
    #[clap(short, long, group("Worker specific arguments"), requires("worker"))]
    authorized_managers: Option<Vec<Libp2pMultiaddr>>,
}

impl std::convert::TryInto<BalthazarConfig> for BalthazarArgs {
    type Error = lib::store::ipfs::IpfsStorageCreationError;

    fn try_into(self) -> Result<BalthazarConfig, Self::Error> {
        let mut config = BalthazarConfig::default();

        if self.is_worker {
            config.set_node_type(NodeType::Worker);

            if let Some(authorized_managers) = self.authorized_managers {
                if let NodeTypeContainer::Worker(ref mut worker_mut) =
                    config.net_mut().node_type_configuration_mut()
                {
                    worker_mut
                        .authorized_managers_mut()
                        .extend_from_slice(&authorized_managers[..]);
                } else {
                    panic!("`authorized_manager` was defined when node type isn't `Worker`.");
                }
            }
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

        Ok(config)
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
