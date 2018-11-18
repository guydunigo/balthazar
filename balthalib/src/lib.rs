extern crate balthacephalo as cephalo;
extern crate balthapode as pode;
extern crate balthernet as net;
extern crate ron;
extern crate tokio;

pub mod config_parser;

use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::convert::From;
use std::fs::File;

use net::asynctest::PeerId;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    CephaloError(cephalo::Error),
    PodeError(pode::Error),
    ArgError(config_parser::ArgError),
    NetError(net::Error),
    TokioRuntimeError,
}

impl From<cephalo::Error> for Error {
    fn from(err: cephalo::Error) -> Error {
        Error::CephaloError(err)
    }
}

impl From<pode::Error> for Error {
    fn from(err: pode::Error) -> Error {
        Error::PodeError(err)
    }
}

impl From<config_parser::ArgError> for Error {
    fn from(err: config_parser::ArgError) -> Error {
        Error::ArgError(err)
    }
}

impl From<net::Error> for Error {
    fn from(err: net::Error) -> Error {
        Error::NetError(err)
    }
}

// ------------------------------------------------------------------

// pub trait Mollusque {
//     // Can't run with tentacles...
//     fn swim(&mut self) -> io::Result<()>;
// }

pub enum CephalopodeType {
    Cephalo,
    Pode,
    InkPode,
    NetTest,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

// ------------------------------------------------------------------

pub fn swim(config: config_parser::Config) -> Result<(), Error> {
    let reader = File::open("./peers.ron").map_err(net::Error::from)?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: actual pid
    let local_pid: PeerId = config.addr.port() as PeerId;
    println!("Using pid : {}", local_pid);

    let mut runtime = Runtime::new().map_err(net::Error::from)?;

    let (shoal, rx) = net::asynctest::shoal::ShoalReadArc::new(local_pid, config.addr);

    shoal.swim(&mut runtime, &addrs[..])?;

    match config.command {
        CephalopodeType::Cephalo => cephalo::swim(&mut runtime, shoal, rx)?,
        CephalopodeType::Pode => pode::swim(&mut runtime, shoal, rx),
        CephalopodeType::InkPode => pode::fill(&mut runtime, shoal, rx)?,
        _ => net::asynctest::swim(&mut runtime, shoal, rx),
    };

    runtime
        .shutdown_on_idle()
        .wait()
        .map_err(|_| Error::TokioRuntimeError)
}
