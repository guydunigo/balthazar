mod cephalo;
pub mod config_parser;
mod pode;

use std::convert::From;
use std::io;

pub use cephalo::Cephalo;
pub use pode::Pode;

#[derive(Debug)]
pub enum CephalopodeError {
    IoError(io::Error),
    ArgError(config_parser::ArgError),
}

impl From<io::Error> for CephalopodeError {
    fn from(err: io::Error) -> CephalopodeError {
        CephalopodeError::IoError(err)
    }
}

impl From<config_parser::ArgError> for CephalopodeError {
    fn from(err: config_parser::ArgError) -> CephalopodeError {
        CephalopodeError::ArgError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
} /* tests */
