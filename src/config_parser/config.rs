use std::net::SocketAddr;

pub enum CephalopodeType {
    Cephalo,
    Pode,
}

pub struct Config {
    pub command: CephalopodeType,
    pub addr: SocketAddr,
}
