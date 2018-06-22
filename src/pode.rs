use std::fmt::Display;
use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};

pub struct Pode {
    cephalo: Option<TcpStream>,
}

impl Pode {
    pub fn new<A: ToSocketAddrs + Display>(addr: A) -> io::Result<Pode> {
        let socket = TcpStream::connect(&addr)?;
        println!("Connected to : `{}`", addr);

        Ok(Pode {
            cephalo: Some(socket),
        })
    }

    pub fn swim(&mut self) -> io::Result<()> {
        if let Some(mut socket) = self.cephalo.take() {
            let mut msg = String::new();
            socket.read_to_string(&mut msg)?;

            println!("Received : `{}`", msg);
        }

        Ok(())
    }
}

impl Drop for Pode {
    fn drop(&mut self) {
        println!("Closing socket...");
        // TODO: Send stop signal before closing
        if let Some(socket) = self.cephalo.take() {
            socket.shutdown(Shutdown::Both).unwrap();
        }
    }
}
