use libp2p::Multiaddr;
use misc::{NodeType, NodeTypeContainer};

pub const DEFAULT_LISTENING_ADDRESS: &str = "/ip4/0.0.0.0/tcp/5003";

pub type NodeTypeConfig = NodeTypeContainer<ManagerConfig, WorkerConfig>;

/// Configuration when the node is a manager.
#[derive(Clone, Default, Debug)]
pub struct ManagerConfig;

/// Configuration when the node is a worker.
#[derive(Clone, Default, Debug)]
pub struct WorkerConfig {
    /// If non-empty, will only accept those addresses as Managers.
    /// If Multiaddr contains an *internet* address, will only accept a connection of it.
    /// If Multiaddr contains a `/p2p/[PEER_ID]` part, will check the `PeerId`.
    authorized_managers: Vec<Multiaddr>,
}

impl WorkerConfig {
    pub fn authorized_managers(&self) -> &Vec<Multiaddr> {
        &self.authorized_managers
    }
    pub fn authorized_managers_mut(&mut self) -> &mut Vec<Multiaddr> {
        &mut self.authorized_managers
    }
}

/// Configuration for the network part.
#[derive(Clone, Debug)]
pub struct NetConfig {
    /// Tcp or Websocket Address to listen on for receiv connections
    listen_addr: Option<Multiaddr>,
    /// Peers to connect to when start up...
    bootstrap_peers: Vec<Multiaddr>,
    node_type_configuration: NodeTypeContainer<ManagerConfig, WorkerConfig>,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            listen_addr: Some(
                DEFAULT_LISTENING_ADDRESS
                    .parse()
                    .expect("Not a valid address"),
            ),
            bootstrap_peers: Vec::new(),
            node_type_configuration: NodeType::default().into(),
        }
    }
}

impl NetConfig {
    pub fn set_node_type(&mut self, node_type: NodeType) {
        let local_node_type: NodeType = self.node_type_configuration().into();
        if local_node_type != node_type {
            self.node_type_configuration = node_type.into();
        }
    }

    pub fn listen_addr(&self) -> &Option<Multiaddr> {
        &self.listen_addr
    }
    pub fn set_listen_addr(&mut self, new: Option<Multiaddr>) {
        self.listen_addr = new;
    }

    pub fn bootstrap_peers(&self) -> &Vec<Multiaddr> {
        &self.bootstrap_peers
    }
    pub fn bootstrap_peers_mut(&mut self) -> &mut Vec<Multiaddr> {
        &mut self.bootstrap_peers
    }

    pub fn node_type_configuration(&self) -> &NodeTypeContainer<ManagerConfig, WorkerConfig> {
        &self.node_type_configuration
    }
    pub fn node_type_configuration_mut(
        &mut self,
    ) -> &mut NodeTypeContainer<ManagerConfig, WorkerConfig> {
        &mut self.node_type_configuration
    }
}
