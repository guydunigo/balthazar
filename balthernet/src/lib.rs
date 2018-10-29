extern crate balthmessage as message;

use std::fmt::Display;
use std::fs::File;
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
    InvalidAddress(String, io::Error),
    CouldNotResolveAddress(String), // TODO: Figure out when this error actually happens
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

pub fn connect_peers(local_addr: &SocketAddr, addrs: &[SocketAddr]) -> Vec<Result<Peer, Error>> {
    let v: Vec<Result<Peer, Error>> = addrs
        .iter()
        .filter(|addr| *addr != local_addr)
        .map(|addr| connect_peer_async(addr.clone()))
        // TODO: the threads should live longer (managers) ?
        .map(|handle| {
            match handle.join() {
                Ok(Ok(res)) => Ok(res),
                Ok(err) => err,
                // TODO: better error?
                Err(_) => Err(Error::ConnectThreadPanicked),
            }
        })
        .collect();

    for (i, res) in v.iter().enumerate() {
        println!("peer #{} : {:?}", i, res);
    }

    v
}

pub fn swim(addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = message::de::from_reader(reader).unwrap();

    let parsed_addrs: Vec<SocketAddr> = addrs
        .iter()
        .map(|addr| parse_socket_addr(addr))
        .filter_map(|addr| match addr {
            Ok(addr) => Some(addr),
            Err(err) => {
                println!("{:?}", err);
                None
            }
        })
        .collect();

    let peers = connect_peers(&addr, &parsed_addrs[..]);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
