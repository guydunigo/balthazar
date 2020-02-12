extern crate balthalib as lib;
extern crate clap;

use std::convert::TryInto;

mod arguments;

use arguments::BalthazarArgs;

pub fn run() {
    let args = BalthazarArgs::parse();
    balthalib::run(args.try_into().unwrap());
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
