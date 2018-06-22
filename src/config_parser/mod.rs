mod config;

use std::convert::From;
use std::env;
use std::io;
use std::net::ToSocketAddrs;

pub use self::config::{CephalopodeType, Config};

#[derive(Debug)]
pub enum ArgError {
    NoCommand,
    NoAddress,
    UnknownCommand(String),
    InvalidAddress(io::Error),
    CouldNotResolveAddress(&'static str),
}

impl From<io::Error> for ArgError {
    fn from(err: io::Error) -> ArgError {
        ArgError::InvalidAddress(err)
    }
}

pub fn parse_config(mut args: env::Args) -> Result<Config, ArgError> {
    args.next();

    let command = match args.next() {
        Some(cmd) => cmd,
        None => return Err(ArgError::NoCommand),
    };

    let command = match &command[..] {
        "c" | "cephalo" => CephalopodeType::Cephalo,
        "p" | "pode" => CephalopodeType::Pode,
        cmd => return Err(ArgError::UnknownCommand(cmd.to_string())),
    };

    let mut addr_iter = match args.next() {
        Some(addr) => addr.to_socket_addrs()?,
        None => return Err(ArgError::NoAddress),
    };

    let addr = match addr_iter.next() {
        Some(addr) => addr,
        None => return Err(ArgError::CouldNotResolveAddress("Invalid given address.")),
    };

    Ok(Config { command, addr })
}
