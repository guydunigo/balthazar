extern crate balthalib;
extern crate balthacli;

use std::env;

use balthalib::{config_parser, cephalo, pode, CephalopodeType};

fn main() -> Result<(), balthalib::Error> {
    let config = config_parser::parse_config(env::args())?;

    match config.command {
        CephalopodeType::Cephalo => {
            let mut cephalo = cephalo::Cephalo::new(config.addr);
            cephalo.swim()?;
        }
        CephalopodeType::Pode => {
            let mut pode = pode::Pode::new(config.addr)?;
            pode.swim()?;
        }
    };

    Ok(())
}
