extern crate balthazar;

use std::env;

use balthazar::config_parser;
use balthazar::mollusque::{cephalo, pode, CephalopodeType};

fn main() -> Result<(), balthazar::Error> {
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
