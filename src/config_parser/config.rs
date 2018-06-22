use mollusque::CephalopodeType;
use std::net::SocketAddr;

pub struct Config {
    pub command: CephalopodeType,
    pub addr: SocketAddr,
}
