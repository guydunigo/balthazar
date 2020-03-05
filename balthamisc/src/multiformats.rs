//! Tools for manipulating multiformats: [`multibase`], [`multihash`], [`multiaddr`].
use super::job::DefaultHash;
use multibase::{decode, encode, Base};
use multihash::Multihash;
use std::fmt;

pub const DEFAULT_BASE: Base = Base::Base64Pad;

#[derive(Debug)]
pub enum Error {
    Multibase(multibase::Error),
    MultihashDecode(multihash::DecodeOwnedError),
    MultihashEncode(multihash::EncodeError),
    WrongHashAlgorithm {
        expected: multihash::Code,
        got: multihash::Code,
    },
    WrongSourceLength {
        expected: usize,
        got: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for Error {}

impl From<multibase::Error> for Error {
    fn from(e: multibase::Error) -> Self {
        Error::Multibase(e)
    }
}

impl From<multihash::DecodeOwnedError> for Error {
    fn from(e: multihash::DecodeOwnedError) -> Self {
        Error::MultihashDecode(e)
    }
}

impl From<multihash::EncodeError> for Error {
    fn from(e: multihash::EncodeError) -> Self {
        Error::MultihashEncode(e)
    }
}

/// Tries decoding the given multibase encoded multihash string.
pub fn try_decode_multibase_multihash_string(src: &str) -> Result<Multihash, Error> {
    let (_, hash) = decode(src)?;
    Ok(Multihash::from_bytes(hash)?)
}

pub fn encode_multibase_multihash_string(hash: &Multihash) -> String {
    let hash = hash.as_bytes();
    encode(DEFAULT_BASE, hash)
}

#[derive(Debug, Clone)]
pub enum RunMode {
    Hash(Vec<u8>),
    Check(Multihash, Vec<u8>),
}

pub fn run(mode: &RunMode) -> Result<(), Error> {
    match mode {
        RunMode::Hash(data) => {
            let hash = DefaultHash::digest(&data[..]);
            let encoded = encode_multibase_multihash_string(&hash);

            eprintln!("Keccak256 hash:");
            println!("{}", encoded);
        }
        RunMode::Check(hash, data) => {
            let algo = hash
                .algorithm()
                .hasher()
                .expect("The provided multihash has been computed with an unknown hash algorithm.");

            let hashed_data = algo.digest(&data[..]);

            if *hash == hashed_data {
                println!("Match");
            } else {
                println!("No match");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_can_encode_decode_string() {
        let src = "MEiCcvAfD+ZFyWDajqipYHKICkZiqQgudmbwOEx2fPiy+Rw==";
        let hash = try_decode_multibase_multihash_string(src).unwrap();
        let dst = encode_multibase_multihash_string(&hash);

        assert_eq!(src, dst);
    }
}
