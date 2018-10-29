extern crate balthmessage as message;

use std::fmt::Display;
use std::io;
use std::net::{AddrParseError, SocketAddr, TcpStream, ToSocketAddrs};
use std::thread;
use std::thread::JoinHandle;

use balthmessage::{Message, MessageReader};

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

#[derive(Debug)]
pub struct Peer {
    pub pid: usize,
    pub addr: SocketAddr,
    pub socket: TcpStream,
}

// TODO: personnal PID (possibly public key)
pub fn connect_peer(addr: SocketAddr) -> Result<Peer, Error> {
    let socket = TcpStream::connect(&addr)?;
    println!("Connected to : `{}`", addr);

    //TODO: as option
    let pid = {
        let mut init_reader = MessageReader::new(0, socket.try_clone()?);
        match init_reader.next() {
            Some(Ok(Message::Connected(pid))) => Ok(pid),
            _ => Err(Error::FailedHandshake),
        }
    }?;
    println!("{} : Handshake successful.", pid);

    Ok(Peer { socket, pid, addr })
}

// TODO: don't use 'static : possible memory loss
pub fn connect_peer_async(addr: SocketAddr) -> JoinHandle<Result<Peer, Error>> {
    thread::spawn(move || connect_peer(addr))
}

pub fn connect_peers(local_addr: SocketAddr, addrs: &[String]) -> Vec<Result<Peer, Error>> {
    let v: Vec<Result<Peer, Error>> = addrs
        .iter()
        .map(|addr| {
            let socket_addr: Result<SocketAddr, AddrParseError> = addr.parse();
            match socket_addr {
                Ok(addr) => Ok(connect_peer_async(addr.clone())),
                Err(err) => Err(Error::from(err)),
            }
        })
        .map(|handle_res| {
            match handle_res {
                Ok(handle) => match handle.join() {
                    Ok(Ok(res)) => Ok(res),
                    Ok(err) => err,
                    // TODO: better error?
                    Err(_) => Err(Error::ConnectThreadPanicked),
                },
                Err(err) => Err(err),
            }
        })
        .collect();

    for (i, res) in v.iter().enumerate() {
        println!("peer #{} : {:?}", i, res);
    }

    v
}

pub fn swim(addr: SocketAddr) -> Result<(), Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
