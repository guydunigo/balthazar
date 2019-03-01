use super::Error;
use net::asynctest::PeerId;
use net::parse_socket_addr;

use std::fs::read_to_string;
use std::net::SocketAddr;

#[derive(Deserialize)]
pub enum CephalopodeType {
    Cephalo,
    Pode,
    InkPode,
    NetTest,
}

/// This is a temporary structure because Serde doesn't parse automatically
/// the `SocketAddr` from `String`.

#[derive(Deserialize)]
struct ConfigFile {
    pub pid: PeerId,
    pub addr: String,
    pub command: CephalopodeType,
    pub peers: Vec<String>,
}

pub struct Config {
    pub pid: PeerId,
    pub addr: SocketAddr,
    pub command: CephalopodeType,
    pub peers: Vec<SocketAddr>,
}

impl Config {
    pub fn from_file(filename: String) -> Result<Self, Error> {
        let config_ser = read_to_string(filename)?;
        let config_parsed: ConfigFile = ron::de::from_str(&config_ser[..])?;

        let res = Config {
            pid: config_parsed.pid,
            command: config_parsed.command,
            addr: parse_socket_addr(config_parsed.addr).map_err(Error::LocalAddressParsingError)?,
            peers: config_parsed
                .peers
                .iter()
                .map(parse_socket_addr)
                .filter_map(|parse_res| match parse_res {
                    Ok(addr) => Some(addr),
                    Err(err) => {
                        eprintln!("Config : Error parsing a peer address : `{:?}`", err);
                        None
                    }
                })
                .collect(),
        };

        Ok(res)
    }
}
