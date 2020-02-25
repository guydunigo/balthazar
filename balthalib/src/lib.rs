pub extern crate balthamisc as misc;
pub extern crate balthastore as store;
pub extern crate balthernet as net;
pub extern crate balthurner as run;
extern crate tokio;

use std::{fmt, io};

use net::identity::error::DecodingError;

mod config;
mod node;
pub use config::{BalthazarConfig, RunMode};

#[derive(Debug)]
pub enum Error {
    KeyPairReadFileError(io::Error),
    KeyPairDecodingError(DecodingError),
    StorageCreationError(store::StoragesWrapperCreationError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self)
    }
}

impl From<store::StoragesWrapperCreationError> for Error {
    fn from(src: store::StoragesWrapperCreationError) -> Self {
        Error::StorageCreationError(src)
    }
}

pub fn run(mode: RunMode, config: BalthazarConfig) -> Result<(), Error> {
    match mode {
        RunMode::Node => node::run(config),
        RunMode::Blockchain => Ok(()),
        RunMode::Storage => Ok(()),
        RunMode::Runner => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
