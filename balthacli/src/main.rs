extern crate balthalib as lib;
extern crate clap;

use clap::Clap;
use std::convert::TryInto;

mod arguments;

use arguments::BalthazarArgs;

fn main() -> Result<(), lib::Error> {
    let args = BalthazarArgs::parse();
    balthalib::run(args.try_into().unwrap())
}
