#![feature(int_to_from_bytes)]

extern crate ron;
extern crate tokio;
// #[macro_use]
extern crate bytes;
extern crate futures;
extern crate rand;

extern crate balthmessage as message;

use std::fmt::Display;
use std::io;
use std::net::{AddrParseError, SocketAddr, TcpStream, ToSocketAddrs};

use balthmessage::{Message, MessageReader};

pub mod asynctest;
pub mod listener;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    FailedHandshake,
    MessageError(message::Error),
    IoError(io::Error),
    ListenerError(listener::Error),
    ConnectThreadPanicked,
    AddrParseError(AddrParseError),
    InvalidAddress(String, io::Error),
    CouldNotResolveAddress(String), // TODO: Figure out when this error actually happens
    TokioRuntimeError,
    TokioTimerError(tokio::timer::Error),
    ConnectionEnded,
    ConnectionCancelled,
    PingSendError,
}

impl From<message::Error> for Error {
    fn from(err: message::Error) -> Error {
        Error::MessageError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<listener::Error> for Error {
    fn from(err: listener::Error) -> Error {
        Error::ListenerError(err)
    }
}

impl From<AddrParseError> for Error {
    fn from(err: AddrParseError) -> Error {
        Error::AddrParseError(err)
    }
}

impl From<tokio::timer::Error> for Error {
    fn from(err: tokio::timer::Error) -> Error {
        Error::TokioTimerError(err)
    }
}

// ------------------------------------------------------------------

pub fn initialize_pode<A: ToSocketAddrs + Display>(addr: A) -> Result<(TcpStream, usize), Error> {
    let socket = TcpStream::connect(&addr)?;
    println!("Connected to : `{}`", addr);

    //TODO: as option
    let pode_id = {
        let mut init_reader = MessageReader::new(0, socket.try_clone()?);
        match init_reader.next() {
            Some(Ok(Message::Connected(pode_id))) => Ok(pode_id),
            _ => Err(Error::FailedHandshake),
        }
    }?;
    println!("{} : Handshake successful.", pode_id);

    Ok((socket, pode_id))
}

// ------------------------------------------------------------------

// TODO: as Async function...
pub fn parse_socket_addr<A: ToSocketAddrs + Display>(addr: A) -> Result<SocketAddr, Error> {
    let addr_opt = match addr.to_socket_addrs() {
        Ok(mut addr_iter) => addr_iter.next(),
        Err(err) => return Err(Error::InvalidAddress(format!("{}", addr), err)),
    };

    match addr_opt {
        Some(addr) => Ok(addr),
        None => Err(Error::CouldNotResolveAddress(format!("{}", addr))),
    }
}

// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
