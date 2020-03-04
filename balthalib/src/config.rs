use chain::{ChainConfig, RunMode as ChainMode};
use net::NetConfig;
use proto::NodeType;
use store::StorageConfig;

const CONFIG_VERSION: &str = "0.1.0";

/// Mode to run.
#[derive(Clone, Debug)]
pub enum RunMode {
    /// Start a full worker or manager node and connect it to the p2p network
    /// (default).
    Node,
    /// Interract with the blockchain.
    Blockchain(ChainMode),
    /// Interract with the storages directly.
    Storage,
    /// Run programs and test them.
    Runner(Vec<u8>, Vec<u8>, usize),
    /// Run balthwasm program natively.
    Native(Vec<u8>, usize),
    /// Misc tools.
    Misc(misc::multiformats::RunMode),
}

impl Default for RunMode {
    fn default() -> Self {
        RunMode::Node
    }
}

/// General configuration for Balthazar.
// #[derive(Clap, Clone, Default, Debug)]
// #[clap(version = "v1.0-beta")]
#[derive(Clone, Debug)]
pub struct BalthazarConfig {
    version: String,
    node_type: NodeType,
    storage: StorageConfig,
    net: NetConfig,
    chain: ChainConfig,
    wasm: Option<(Vec<u8>, Vec<Vec<u8>>)>,
}

impl Default for BalthazarConfig {
    fn default() -> Self {
        BalthazarConfig {
            version: CONFIG_VERSION.to_string(),
            node_type: NodeType::default(),
            storage: StorageConfig::default(),
            net: NetConfig::default(),
            chain: ChainConfig::default(),
            wasm: None,
        }
    }
}

impl BalthazarConfig {
    pub fn version(&self) -> &str {
        &self.version[..]
    }

    pub fn node_type(&self) -> &NodeType {
        &self.node_type
    }
    pub fn set_node_type(&mut self, new: NodeType) {
        self.node_type = new;
        self.net_mut().set_node_type(new);
    }

    pub fn storage(&self) -> &StorageConfig {
        &self.storage
    }
    pub fn storage_mut(&mut self) -> &mut StorageConfig {
        &mut self.storage
    }

    pub fn net(&self) -> &NetConfig {
        &self.net
    }
    pub fn net_mut(&mut self) -> &mut NetConfig {
        &mut self.net
    }

    pub fn chain(&self) -> &ChainConfig {
        &self.chain
    }
    pub fn chain_mut(&mut self) -> &mut ChainConfig {
        &mut self.chain
    }

    pub fn wasm(&self) -> &Option<(Vec<u8>, Vec<Vec<u8>>)> {
        &self.wasm
    }
    pub fn set_wasm(&mut self, new: Option<(Vec<u8>, Vec<Vec<u8>>)>) {
        self.wasm = new;
    }
}
