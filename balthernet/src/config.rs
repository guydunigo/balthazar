use libp2p::{core::multiaddr::Protocol, Multiaddr, PeerId};
use proto::{NodeType, NodeTypeContainer};
use std::time::Duration;

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

    /// Check through `authorized_managers` to see if one match.
    /// `authorized_managers` elements containing a PeerId or an address or both,
    /// for at least one element, the parameters must contain at least the same components
    /// and they must match.
    pub fn is_manager_authorized(&self, peer_id: Option<&PeerId>, addrs: &[Multiaddr]) -> bool {
        if self.authorized_managers.is_empty() {
            // If it's empty, everyone is accepted.
            true
        } else {
            self.authorized_managers.iter().any(|a| {
                let mut a_clone = a.clone();
                // If we have a PeerId at the end of the multihash (`.../p2p/[MULTIHASH]`).
                if let Some(Protocol::P2p(multihash)) = a_clone.pop() {
                    // If a PeerId was provided by argument.
                    if let Some(peer_id) = peer_id {
                        // If a PeerId can be matched after being extracted from addr.
                        if let Ok(a_peer_id) = PeerId::from_multihash(multihash) {
                            // PeerIds must match
                            // If `a` contains an address, one of the addrs must match.
                            &a_peer_id == peer_id
                                && (a_clone.iter().next().is_none()
                                    || addrs.iter().any(|addr| &a_clone == addr))
                        } else {
                            // If a PeerId can't be matched after being extracted from addr,
                            // nothing can match it.
                            false
                        }
                    } else {
                        // No peer_id was provided but one was found in `a`.
                        false
                    }
                } else {
                    // No peer_id in `a` so it all goes back to comparing addresses.
                    addrs.iter().any(|addr| a == addr)
                }
            })
        }
    }
}

/// Configuration for the network part.
#[derive(Clone, Debug)]
pub struct NetConfig {
    /// Tcp or Websocket Address to listen on for receiv connections
    listen_addr: Option<Multiaddr>,
    /// Peers to connect to when start up...
    bootstrap_peers: Vec<Multiaddr>,
    // TODO: good idea to have duplicate node type ?
    /// Configuration relative to node type.
    node_type_configuration: NodeTypeContainer<ManagerConfig, WorkerConfig>,
    /// Duration after the last [`ManagerPong`](`proto::worker::ManagerPong`) to send
    /// the next [`ManagerPing`](`proto::worker::ManagerPing`) to our manager/worker
    /// if we are in a relationship.
    /// This helps check the manager/worker is still up and running and able to
    /// manage/be managed by us.
    /// The interval is reset after each successful [`ManagerPong`](`proto::worker::ManagerPong`) message
    /// received.
    manager_check_interval: Duration,
    /// Maximum interval to wait after a [`ManagerPing`](`proto::worker::ManagerPong`)
    /// to receive a [`ManagerPong`](`proto::worker::ManagerPong`) from the worker's
    /// manager (if we are in a relationship).
    manager_timeout: Duration,
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
            manager_check_interval: Duration::from_secs(15),
            manager_timeout: Duration::from_secs(60),
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

    pub fn bootstrap_peers(&self) -> &[Multiaddr] {
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

    pub fn manager_check_interval(&self) -> &Duration {
        &self.manager_check_interval
    }
    pub fn set_manager_check_interval(&mut self, new: Duration) {
        self.manager_check_interval = new;
    }

    pub fn manager_timeout(&self) -> &Duration {
        &self.manager_timeout
    }
    pub fn set_manager_timeout(&mut self, new: Duration) {
        self.manager_timeout = new;
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerConfig;
    use libp2p::{core::multiaddr::Protocol, Multiaddr, PeerId};

    #[test]
    fn it_correctly_checks_managers_authorized_empty() {
        let conf = WorkerConfig::default();
        let res = conf.is_manager_authorized(None, &[]);

        assert_eq!(res, true);
    }

    // TODO: better, more exhaustive tests
    #[test]
    fn it_correctly_checks_managers_authorized() {
        let mut conf = WorkerConfig::default();
        let mut peer_id: Multiaddr = "/p2p/QmdenMRzgF5SVBThqQzhe8usVsoBrnn7Y2Vg33GooVDuyf"
            .parse()
            .unwrap();

        conf.authorized_managers_mut()
            .push("/ip4/127.0.0.1/tcp/3333".parse().unwrap());
        conf.authorized_managers_mut().push(peer_id.clone());

        let res_2 = if let Some(Protocol::P2p(multihash)) = peer_id.pop() {
            let peer_id = PeerId::from_multihash(multihash).unwrap();
            conf.is_manager_authorized(Some(&peer_id), &[])
        } else {
            unreachable!();
        };

        let res_0 = conf.is_manager_authorized(None, &["/ip4/127.0.0.1/tcp/3333".parse().unwrap()]);
        let res_1 = conf.is_manager_authorized(None, &["/ip4/127.0.0.1/tcp/3334".parse().unwrap()]);

        assert_eq!(res_0, true);
        assert_eq!(res_1, false);
        assert_eq!(res_2, true);
    }
}
