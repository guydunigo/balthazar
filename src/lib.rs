pub mod config_parser;
pub mod mollusque;

use mollusque::{cephalo, pode, CephalopodeType};
use std::convert::From;

#[derive(Debug)]
pub enum Error {
    CephaloError(cephalo::Error),
    PodeError(pode::Error),
    ArgError(config_parser::ArgError),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
} /* tests */
