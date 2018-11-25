extern crate balthacephalo as cephalo;
extern crate balthapode as pode;
extern crate balthernet as net;

#[macro_use]
extern crate serde_derive;
extern crate ron;
extern crate serde;
extern crate tokio;

pub mod config_parser;

use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::convert::From;
use std::io;

use config_parser::{CephalopodeType, Config};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    CephaloError(cephalo::Error),
    PodeError(pode::Error),
    ArgError(config_parser::ArgError),
    NetError(net::Error),
    TokioRuntimeError,
    IoError(io::Error),
    ConfigFileDeserializationError(ron::de::Error),
    LocalAddressParsingError(net::Error),
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

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<ron::de::Error> for Error {
    fn from(err: ron::de::Error) -> Error {
        Error::ConfigFileDeserializationError(err)
    }
}

// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

// ------------------------------------------------------------------

pub fn swim(config: Config) -> Result<(), Error> {
    println!("Using pid : {}", config.pid);

    let mut runtime = Runtime::new().map_err(net::Error::from)?;

    let (shoal, rx) = net::asynctest::shoal::ShoalReadArc::new(config.pid, config.addr);

    shoal.swim(&mut runtime, &config.peers[..])?;

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
