extern crate balthacli;
extern crate balthalib;

use std::env;

use balthalib::{cephalo, config_parser, pode, CephalopodeType};

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
