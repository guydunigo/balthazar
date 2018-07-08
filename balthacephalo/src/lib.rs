extern crate balthmessage as message;

mod listener;
mod orchestrator;

use std::convert::From;
use std::fmt::Display;
use std::net::ToSocketAddrs;
use std::sync::mpsc;
use std::thread;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    ListenerError(listener::Error),
    OrchestratorError(orchestrator::Error),
}

impl From<listener::Error> for Error {
    fn from(err: listener::Error) -> Error {
        Error::ListenerError(err)
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

    orchestrator::orchestrate(rx)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
