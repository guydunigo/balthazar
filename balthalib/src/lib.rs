pub extern crate balthachain as chain;
pub extern crate balthamisc as misc;
pub extern crate balthaproto as proto;
pub extern crate balthastore as store;
pub extern crate balthernet as net;
pub extern crate balthurner as run;
extern crate balthwasm as wasm;
extern crate futures;
extern crate tokio;

use futures::channel::mpsc::SendError;
use std::{fmt, io};

use net::identity::error::DecodingError;

mod config;
mod native;
mod node;
pub use config::{BalthazarConfig, RunMode};
use misc::multiformats as formats;

#[derive(Debug)]
pub enum Error {
    KeyPairReadFileError(io::Error),
    KeyPairDecodingError(DecodingError),
    StorageCreationError(store::StoragesWrapperCreationError),
    RunnerError(run::RunnerError<run::wasm::Error>),
    ChainError(chain::Error),
    NativeError(i64),
    MiscError(formats::Error),
    EventChannelError(SendError),
    SwarmChannelError(SendError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self)
    }
}

impl std::error::Error for Error {}

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

impl From<formats::Error> for Error {
    fn from(src: formats::Error) -> Self {
        Error::MiscError(src)
    }
}

impl From<SendError> for Error {
    fn from(src: SendError) -> Self {
        Error::EventChannelError(src)
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
        RunMode::Native(args, nb_times) => {
            native::run(args, nb_times).map_err(Error::NativeError)?
        }
        RunMode::Misc(mode) => misc::multiformats::run(&mode)?,
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
