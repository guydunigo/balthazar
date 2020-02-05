extern crate balthalib as lib;

use lib::{misc::NodeType, net::Multiaddr};
use std::env;

pub fn run() {
    let elm_type = env::args().nth(1).unwrap_or_else(|| "0".to_string());
    let dial_addrs = env::args().skip(2);

    let node_type = match &elm_type[..] {
        "0" => NodeType::Manager,
        _ => NodeType::Worker(()),
    };

    println!("Starting as {:?}...", node_type);

    let listen_addr: Multiaddr = if let NodeType::Manager = node_type {
        "/ip4/0.0.0.0/tcp/3333"
    } else {
        "/ip4/0.0.0.0/tcp/0"
    }
    .parse()
    .expect("invalid multiaddr");

    let addresses_to_dial = dial_addrs
        .map(|addr| addr.parse().expect("invalid dial address parameter"))
        .collect::<Vec<Multiaddr>>();

    balthalib::run(node_type, listen_addr, &addresses_to_dial[..]);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
