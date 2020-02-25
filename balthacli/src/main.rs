extern crate balthalib as lib;
extern crate clap;

use clap::Clap;
use std::convert::TryInto;

mod arguments;

use arguments::BalthazarArgs;

fn main() -> Result<(), lib::Error> {
    let (mode, args) = BalthazarArgs::parse().try_into().unwrap();
    balthalib::run(mode, args)
}
