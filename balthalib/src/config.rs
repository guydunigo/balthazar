use misc::NodeType;
use net::NetConfig;
use store::StorageConfig;

const CONFIG_VERSION: &str = "0.1.0";

/// General configuration for Balthazar.
// #[derive(Clap, Clone, Default, Debug)]
// #[clap(version = "v1.0-beta")]
#[derive(Clone, Debug)]
pub struct BalthazarConfig {
    version: String,
    node_type: NodeType,
    storage: StorageConfig,
    net: NetConfig,
}

impl Default for BalthazarConfig {
    fn default() -> Self {
        BalthazarConfig {
            version: CONFIG_VERSION.to_string(),
            node_type: NodeType::default(),
            storage: StorageConfig::default(),
            net: NetConfig::default(),
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
}
