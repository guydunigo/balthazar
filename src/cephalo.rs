// use std::io::prelude::*;
use std::fmt::Display;
use std::io;
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};

pub struct Cephalo {
    pods: Vec<TcpStream>,
    listener: TcpListener,
}

impl Cephalo {
    pub fn new<A: ToSocketAddrs + Display>(listen_addr: A) -> io::Result<Cephalo> {
        println!("Listening on : `{}`", listen_addr);
        // TODO: look at bind args (the format might be useless)
        // TODO: Look at different errors
        let listener = TcpListener::bind(listen_addr)?;

        Ok(Cephalo {
            pods: Vec::new(),
            listener,
        })
    }

    pub fn swim(&mut self) -> io::Result<()> {
        for stream in self.listener.incoming() {
            // TODO: Look at different errors
            let stream = stream?;
            let peer_addr = stream.peer_addr()?;
            println!("New peer at address : `{}`", peer_addr);

            self.pods.push(stream);
        }

        Ok(())
    }
}

impl Drop for Cephalo {
    fn drop(&mut self) {
        println!("Closing sockets...");

        while let Some(stream) = self.pods.pop() {
            // TODO: Send stop signal before closing
            stream.shutdown(Shutdown::Both).unwrap();
        }
    }
}
