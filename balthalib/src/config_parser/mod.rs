mod config;

use std::env;

pub use self::config::{CephalopodeType, Config};
pub use super::Error;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum ArgError {
    NoConfigFile,
}

// ------------------------------------------------------------------

pub fn parse_config(mut args: env::Args) -> Result<Config, Error> {
    args.next();

    let conf_filename = match args.next() {
        Some(cmd) => cmd,
        None => return Err(Error::from(ArgError::NoConfigFile)),
    };

    println!("Using config file : `{}`.", conf_filename);

    Config::from_file(conf_filename)
}
