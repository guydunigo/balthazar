use libp2p::Multiaddr;

pub const DEFAULT_LISTENING_ADDRESS: &str = "/ip4/0.0.0.0/tcp/5003";

#[derive(Clone, Debug)]
pub struct NetConfig {
    /// Tcp or Websocket Address to listen on for receiv connections
    listen_addr: Option<Multiaddr>,
    /// Peers to connect to when start up...
    bootstrap_peers: Vec<Multiaddr>,
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
        }
    }
}

impl NetConfig {
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
}
