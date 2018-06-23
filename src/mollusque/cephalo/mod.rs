use super::Mollusque;
use std::fmt::Display;
use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

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
}

impl Mollusque for Cephalo {
    fn swim(&mut self) -> io::Result<()> {
        /*for stream in self.listener.incoming() {
            // TODO: Look at different errors
            let mut stream = stream?;
            let peer_addr = stream.peer_addr()?;
            println!("New peer at address : `{}`", peer_addr);

            self.pods.push(stream);
            println!("Size of the pods list : {}", self.pods.len());
        }*/

        self.listener
            .incoming()
            .map(|stream| -> io::Result<()> {
                // TODO: Look at different errors
                let mut stream = stream?;
                let peer_addr = stream.peer_addr()?;
                println!("New peer at address : `{}`", peer_addr);

                thread::sleep(Duration::from_secs(5));
                let buffer = b"test";
                stream.write_all(buffer)?;
                stream.flush()?;

                // let mut msg = String::new();
                // stream.read_to_string(&mut msg)?;

                // println!("Received : `{}`", msg);

                // let mut buffer: [u8; 512] = [0; 512];
                // stream.read(&mut buffer);

                Ok(())
            })
            .skip_while(|result| result.is_ok())
            .next()
            .unwrap()?;
        //.for_each(|res| eprintln!("{:?}", res));

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
