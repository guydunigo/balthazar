extern crate balthazar;

use std::env;

use balthazar::{Cephalo, Pode, CephalopodeError};
use balthazar::config_parser;
use balthazar::config_parser::CephalopodeType;

fn main() -> Result<(), CephalopodeError> {
    let config = config_parser::parse_config(env::args())?;

    match config.command {
        CephalopodeType::Cephalo => {
            let mut c = Cephalo::new(config.addr)?;
            
            c.swim()
        },
        CephalopodeType::Pode => {
            let mut p = Pode::new(config.addr)?;

            p.swim()
        },
    }?;

    Ok(())
}
