extern crate balthazar;

use std::env;

use balthazar::BalthazarError;
use balthazar::{choose_mollusque, config_parser};

fn main() -> Result<(), BalthazarError> {
    let config = config_parser::parse_config(env::args())?;

    let mut mollusque = choose_mollusque(config)?;

    mollusque.swim()?;

    Ok(())
}
