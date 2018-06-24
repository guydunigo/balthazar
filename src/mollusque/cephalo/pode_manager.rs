use std::io;
use std::io::prelude::*;
use std::sync::mpsc::Receiver;
use std::net::TcpStream;

pub fn manage(rx: Receiver<TcpStream>) -> io::Result<()> {
}
