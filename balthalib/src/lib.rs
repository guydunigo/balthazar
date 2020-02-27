pub extern crate balthachain as chain;
pub extern crate balthamisc as misc;
pub extern crate balthaproto as proto;
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
    RunnerError(run::RunnerError<run::wasm::Error>),
    ChainError(chain::Error),
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

impl From<run::RunnerError<run::wasm::Error>> for Error {
    fn from(src: run::RunnerError<run::wasm::Error>) -> Self {
        Error::RunnerError(src)
    }
}

impl From<chain::Error> for Error {
    fn from(src: chain::Error) -> Self {
        Error::ChainError(src)
    }
}

pub fn run(mode: RunMode, config: BalthazarConfig) -> Result<(), Error> {
    match mode {
        RunMode::Node => node::run(config)?,
        RunMode::Blockchain(mode) => chain::run(&mode, config.chain())?,
        RunMode::Storage => {}
        RunMode::Runner(wasm_file_path, args, nb_times) => {
            run::run(wasm_file_path, args, nb_times)?
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
