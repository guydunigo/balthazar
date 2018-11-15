pub extern crate balthacephalo as cephalo;
pub extern crate balthapode as pode;
pub extern crate balthernet as net;

pub mod config_parser;

use std::convert::From;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    CephaloError(cephalo::Error),
    PodeError(pode::Error),
    ArgError(config_parser::ArgError),
    NetError(net::Error),
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
    match config.command {
        CephalopodeType::Cephalo => cephalo::swim(config.addr)?,
        CephalopodeType::Pode => pode::swim(config.addr)?,
        CephalopodeType::InkPode => pode::fill(config.addr)?,
        CephalopodeType::NetTest => net::asynctest::swim(config.addr)?,
    };

    Ok(())
}
