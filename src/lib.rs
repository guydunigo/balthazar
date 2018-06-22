pub mod config_parser;
pub mod mollusque;

use mollusque::{Cephalo, CephalopodeType, Mollusque, Pode};
use std::convert::From;
use std::io;

#[derive(Debug)]
pub enum BalthazarError {
    IoError(io::Error),
    ArgError(config_parser::ArgError),
}

impl From<io::Error> for BalthazarError {
    fn from(err: io::Error) -> BalthazarError {
        BalthazarError::IoError(err)
    }
}

impl From<config_parser::ArgError> for BalthazarError {
    fn from(err: config_parser::ArgError) -> BalthazarError {
        BalthazarError::ArgError(err)
    }
}

pub fn choose_mollusque(config: config_parser::Config) -> Result<Box<Mollusque>, BalthazarError> {
    match config.command {
        CephalopodeType::Cephalo => Ok(Box::new(Cephalo::new(config.addr)?)),
        CephalopodeType::Pode => Ok(Box::new(Pode::new(config.addr)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {}
} /* tests */
