extern crate sha3;

use sha3::{Digest, Sha3_256};

extern "C" {
    fn push_char(s: u8);
    fn return_256bits(s: [u8; 32]);
}

#[no_mangle]
pub extern "C" fn start() -> u32 {
    let res = add(3, 2);

    let mut hasher = Sha3_256::default();
    //hasher.input(format!("{}", res).as_bytes());
    hasher.input(b"test");

    let hash = hasher.result();

    let mut array_hash: [u8; 32] = [0; 32];

    for i in 0..32 {
        array_hash[i] = hash[i];
    }

    unsafe {
        return_256bits(array_hash);
        let string = String::from("test");
        string
            .into_bytes()
            .iter()
            .for_each(|c| push_char(c.clone()));
    }

    res
}

fn add(x: u32, y: u32) -> u32 {
    x + y
}
