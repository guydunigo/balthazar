extern crate generic_array;
extern crate sha3;

use generic_array::{typenum::U32, GenericArray};
use sha3::{Digest, Sha3_256};

// extern "C" {
//     fn log(s: &str);
// }

// #[no_mangle]
// pub unsafe extern "C" fn greet(name: &str) {
//     log(format!("Hello {}!", String::from(name)).as_str());
// }
//
// #[no_mangle]
// pub unsafe extern "C" fn len(name: *mut c_char) -> usize {
//     CString::from_raw(name).into_bytes().len()
// }

#[no_mangle]
pub extern "C" fn start() -> u32 {
    let res = add(3, 2);

    let mut hasher = Sha3_256::default();
    hasher.input(format!("{}", res).as_bytes());

    let out = hasher.result();

    //out[0] as u32
    digest_to_u32(out)
}

fn digest_to_u32(digest: GenericArray<u8, U32>) -> u32 {
    let mut res = 0;

    for i in 0..4 {
        res = res * 256 + digest[i] as u32;
    }

    res
}

fn add(x: u32, y: u32) -> u32 {
    x + y
}
