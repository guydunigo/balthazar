use std::convert::From;
use std::fmt::Display;
use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};

const BUF_SIZE: usize = 1024;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

pub struct Pode {
    cephalo: Option<TcpStream>,
}

impl Pode {
    pub fn new<A: ToSocketAddrs + Display>(addr: A) -> Result<Pode, Error> {
        let socket = TcpStream::connect(&addr)?;
        println!("Connected to : `{}`", addr);

        Ok(Pode {
            cephalo: Some(socket),
        })
    }

    pub fn swim(&mut self) -> Result<(), Error> {
        if let Some(mut socket) = self.cephalo.take() {
            let mut msg = [0; BUF_SIZE];
            let n = socket.read(&mut msg)?;

            let str_msg = String::from_utf8_lossy(&msg[..n]);
            println!("Received : `{}`", str_msg);

            let answer = format!(
                "local: {} - server: {}",
                socket.local_addr().unwrap(),
                socket.peer_addr().unwrap()
            );
            socket.write_all(answer.as_bytes())?;
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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
