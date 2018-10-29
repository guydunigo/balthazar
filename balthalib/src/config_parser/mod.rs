mod config;

use net;
use std::env;

pub use self::config::Config;
use super::CephalopodeType;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum ArgError {
    NoCommand,
    NoAddress,
    UnknownCommand(String),
    InvalidAddress(net::Error),
}

// ------------------------------------------------------------------

pub fn parse_config(mut args: env::Args) -> Result<Config, ArgError> {
    args.next();

    let command = match args.next() {
        Some(cmd) => cmd,
        None => return Err(ArgError::NoCommand),
    };

    let command = match &command[..] {
        "c" | "cephalo" => CephalopodeType::Cephalo,
        "p" | "pode" => CephalopodeType::Pode,
        "i" | "inkpode" => CephalopodeType::InkPode,
        "n" | "netTest" => CephalopodeType::NetTest,
        cmd => return Err(ArgError::UnknownCommand(cmd.to_string())),
    };

    let addr_res = match args.next() {
        Some(addr) => net::parse_socket_addr(addr),
        None => return Err(ArgError::NoAddress),
    };

    let addr = match addr_res {
        Ok(addr) => addr,
        Err(err) => return Err(ArgError::InvalidAddress(err)),
    };

    Ok(Config { command, addr })
}
