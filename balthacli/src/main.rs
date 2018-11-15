extern crate balthacli as cli;
extern crate balthalib as lib;
use lib::config_parser::parse_config;

use std::env;

fn main() -> Result<(), balthalib::Error> {
    let config = parse_config(env::args())?;
    lib::swim(config)
}
