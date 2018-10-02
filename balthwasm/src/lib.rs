#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;
extern crate sha3;

use ron::ser;

use sha3::{Digest, Sha3_256};

extern "C" {
    fn push_byte(s: u8);
    fn get_byte() -> u8;
    fn get_bytes_len() -> u64;
}

#[derive(Serialize)]
struct ReturnValue {
    bytes: Vec<u8>,
    hash: [u8; 32],
}

#[no_mangle]
pub extern "C" fn start() {
    let mut input = Vec::new();

    unsafe {
        let bytes_len = get_bytes_len() as u32;
        for _ in 0..bytes_len {
            input.push(get_byte());
        }
    }

    let mut hasher = Sha3_256::default();
    hasher.input(&input[..]);

    let hash = hasher.result();

    let mut array_hash: [u8; 32] = [0; 32];

    for i in 0..32 {
        array_hash[i] = hash[i];
    }

    let retval = ReturnValue {
        bytes: input,
        hash: array_hash,
    };
    let ron_retval = ser::to_string(&retval).unwrap();
    unsafe {
        ron_retval
            .into_bytes()
            .iter()
            .for_each(|c| push_byte(c.clone()));
    }
}
