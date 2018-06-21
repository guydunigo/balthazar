extern crate balthazar;

use std::env;
use std::net::ToSocketAddrs;

use balthazar::{Cephalo, Pode, CephalopodeType, CephalopodeError};

fn main() -> Result<(), CephalopodeError> {
    let args = env::args();
    args.next();

    let command = match args.next() {
        Some("c") | Some("cephalo") => CephalopodeType::Cephalo,
        Some("p") | Some("pode") => CephalopodeType::Pode,
        Some(cmd) => return Err(format!("Unknown command : `{}`", cmd)),
        None => return Err("No command provided !"),
    };

    let addr = match args.next() {
        Some(addr) => addr.to_socket_addrs()?,
        None => return Err("No addr to connect to or listen on"),
    };

    match command {
        CephalopodeType::Cephalo => {
            let mut c = Cephalo::new(addr)?;
            
            c.swim()
        },
        CephalopodeType::Pode => {
            let mut p = Pode::new(addr)?;

            p.swim()
        },
    };

    Ok(())
}
