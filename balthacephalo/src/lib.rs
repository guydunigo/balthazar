extern crate balthmessage as message;
extern crate balthajob as job;

mod listener;
mod orchestrator;

use job::Job;
use std::convert::From;
use std::fmt::Display;
use std::net::ToSocketAddrs;
use std::sync::mpsc;
use std::thread;
use std::io;

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
    let jobs: Vec<Job> = Vec::new();

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
