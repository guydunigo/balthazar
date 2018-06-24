mod pode_listener;

use std::convert::From;
use std::fmt::Display;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::thread;

#[derive(Debug)]
pub enum Error {
    ThreadPanicked,
    ListenerError(pode_listener::Error),
}

impl From<pode_listener::Error> for Error {
    fn from(err: pode_listener::Error) -> Error {
        Error::ListenerError(err)
    }
}

pub struct Cephalo<A: 'static + ToSocketAddrs + Display + Send> {
    pods: Vec<TcpStream>,
    listen_addr: Option<A>,
}

impl<A: 'static + ToSocketAddrs + Display + Send> Cephalo<A> {
    pub fn new(listen_addr: A) -> Cephalo<A> {
        Cephalo {
            pods: Vec::new(),
            listen_addr: Some(listen_addr),
        }
    }

    pub fn swim(&mut self) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel();

        let listen_addr = self.listen_addr.take().unwrap(); // safe to unwrap, normally...
                                                            // TODO: name threads
        let listen_handle = thread::spawn(move || -> Result<(), pode_listener::Error> {
            pode_listener::listen(listen_addr, tx)
        });

        match listen_handle.join() {
            Err(_) => return Err(Error::ThreadPanicked),
            Ok(Err(err)) => return Err(Error::from(err)),
            _ => (),
        };

        Ok(())
    }
}

impl<A: 'static + ToSocketAddrs + Display + Send> Drop for Cephalo<A> {
    fn drop(&mut self) {
        println!("Closing sockets...");

        while let Some(stream) = self.pods.pop() {
            // TODO: Send stop signal before closing
            stream.shutdown(Shutdown::Both).unwrap();
        }
    }
}
