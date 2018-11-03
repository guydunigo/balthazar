extern crate balthacli;
extern crate balthalib;

use std::env;

use balthalib::{cephalo, config_parser, net, pode, CephalopodeType};

fn main() -> Result<(), balthalib::Error> {
    let config = config_parser::parse_config(env::args())?;

    match config.command {
        CephalopodeType::Cephalo => cephalo::swim(config.addr)?,
        CephalopodeType::Pode => pode::swim(config.addr)?,
        CephalopodeType::InkPode => pode::fill(config.addr)?,
        CephalopodeType::NetTest => net::asynctest::swim(config.addr)?,
    };

    Ok(())
}
