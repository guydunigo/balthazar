extern crate balthmessage as message;

mod listener;
mod orchestrator;

use std::convert::From;
use std::fmt::Display;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::net::ToSocketAddrs;
use std::sync::mpsc;
use std::thread;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    ListenerError(listener::Error),
    OrchestratorError(orchestrator::Error),
    IoError(io::Error),
    ThreadPanicked,
}

impl From<listener::Error> for Error {
    fn from(err: listener::Error) -> Error {
        Error::ListenerError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<orchestrator::Error> for Error {
    fn from(err: orchestrator::Error) -> Error {
        Error::OrchestratorError(err)
    }
}

// ------------------------------------------------------------------

// TODO: name threads
pub fn swim<A: 'static + ToSocketAddrs + Display + Send>(listen_addr: A) -> Result<(), Error> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || -> Result<(), listener::Error> { listener::listen(listen_addr, tx) });

    let mut jobs: Vec<Vec<u8>> = Vec::new();
    let mut f = File::open("../hello_world.wasm")?;
    let mut code: Vec<u8> = Vec::new();
    f.read_to_end(&mut code)?;
    jobs.push(code);

    orchestrator::orchestrate(rx, jobs)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
