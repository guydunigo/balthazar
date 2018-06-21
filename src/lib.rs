mod cephalo;
mod pode;

use std::io;
use std::convert::From;

pub use cephalo::Cephalo;
pub use pode::Pode;

pub enum CephalopodeType {
    Cephalo,
    Pode,
}

pub enum CephalopodeError {
    IoError(io::Error),
    ArgError(&'static str),
}

impl From<io::Error> for CephalopodeError {
    fn from(err: io::Error) -> CephalopodeError {
        CephalopodeError::IoError(err)
    }
}

impl From<&'static str> for CephalopodeError {
    fn from(err: &'static str) -> CephalopodeError {
        CephalopodeError::ArgError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
} /* tests */
