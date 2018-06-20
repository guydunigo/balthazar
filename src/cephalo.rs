// use std::io::prelude::*;
use std::io;
use std::net::{TcpListener, TcpStream};

const LISTEN_IP: &str = "127.0.0.1";

pub struct Cephalo {
    port: u16,
    pods: Vec<TcpStream>,
}

impl Cephalo {
    pub fn new(port: u16) -> Cephalo {
        Cephalo {
            port,
            pods: Vec::new(),
        }
    }

    // Can't run with tentacles...
    pub fn swin(&mut self) -> io::Result<()> {
        let listen_addr = format!("{}:{}", LISTEN_IP, self.port);

        println!("Listening on : `{}`", listen_addr);
        // TODO: look at bind args (the format might be useless)
        // TODO: Look at different errors
        let listener = TcpListener::bind(listen_addr)?;

        for stream in listener.incoming() {
            // TODO: Look at different errors
            let stream = stream?;
            let peer_addr = stream.peer_addr()?;
            println!("Add peer at address : `{}`", peer_addr);

            self.pods.push(stream);
        }

        Ok(())
    }
}
