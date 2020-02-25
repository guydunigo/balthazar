use misc::NodeType;
use net::NetConfig;
use std::path::PathBuf;
use store::StorageConfig;

const CONFIG_VERSION: &str = "0.1.0";

/// Mode to run.
#[derive(Clone, Debug)]
pub enum RunMode {
    /// Start a full worker or manager node and connect it to the p2p network
    /// (default).
    Node,
    /// Interract with the blockchain.
    Blockchain,
    /// Interract with the storages directly.
    Storage,
    /// Run programs and test them.
    Runner(PathBuf, Vec<u8>, usize),
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
    wasm: Option<(Vec<u8>, Vec<u8>)>,
}

impl Default for BalthazarConfig {
    fn default() -> Self {
        BalthazarConfig {
            version: CONFIG_VERSION.to_string(),
            node_type: NodeType::default(),
            storage: StorageConfig::default(),
            net: NetConfig::default(),
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

    pub fn wasm(&self) -> &Option<(Vec<u8>, Vec<u8>)> {
        &self.wasm
    }
    pub fn wasm_mut(&mut self) -> &mut Option<(Vec<u8>, Vec<u8>)> {
        &mut self.wasm
    }
}
