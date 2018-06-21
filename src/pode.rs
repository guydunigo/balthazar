// use std::io::prelude::*;
use std::io;
use std::net::{TcpStream, ToSocketAddrs, Shutdown};

pub struct Pode {
    socket: TcpStream,
}

impl Pode {
    pub fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Pode>  {
        let socket = TcpStream::connect(addr)?;
        Ok(Pode {
            socket,
        })
    }
}

impl Drop for Pode {
    fn drop(&mut self) {
        self.socket.shutdown(Shutdown::Both).unwrap();
    }
}
