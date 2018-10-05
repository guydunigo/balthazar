#[macro_use]
extern crate serde_derive;

extern crate balthajob as job;
use balthajob::task::arguments::Arguments;

extern crate ron;
extern crate serde;
extern crate sha3;

use ron::{de, ser};
use sha3::{Digest, Sha3_256};

use std::fs::File;
use std::io::Read;
use std::io::Write;

#[derive(Serialize)]
struct ReturnValue {
    bytes: Vec<u8>,
    hash: [u8; 32],
}

fn hash(input: &[u8]) -> Vec<u8> {
    let mut hasher = Sha3_256::default();
    hasher.input(input);

    let hash = hasher.result();

    let mut array_hash: [u8; 32] = [0; 32];

    for i in 0..32 {
        array_hash[i] = hash[i];
    }

    let retval = ReturnValue {
        bytes: Vec::from(input),
        hash: array_hash,
    };
    let ron_retval = ser::to_string(&retval).unwrap();
    ron_retval.into_bytes()
}

fn main() {
    let mut res_file = File::create("./res.ron").unwrap();
    let mut f = File::open("./args_list.ron").unwrap();
    let mut buffer = Vec::new();
    f.read_to_end(&mut buffer).unwrap();

    let argslist: Vec<Arguments> = de::from_bytes(&buffer[..]).unwrap();

    argslist.iter().for_each(|args| {
        let res = hash(&args.bytes[..]);
        res_file.write_all(&res[..]).unwrap();
    });
}
