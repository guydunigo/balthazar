extern crate multihash;

use multihash::Keccak256;

mod abi;
use abi::LocalResult;

// ------------------------------------------------------------------
//                          User's code
// ------------------------------------------------------------------

pub fn my_run(arguments: Vec<u8>) -> LocalResult<Vec<u8>> {
    // TODO: checking arguments.

    for counter in 0.. {
        let string = format!("{}", counter);
        let hash_bytes = Keccak256::digest(string.as_bytes()).into_bytes();
        let hash: String = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect();

        if hash
            .as_bytes()
            .windows(arguments.len())
            .any(|w| *w == arguments[..])
        {
            let return_string = format!("({}, {})", counter, hash);
            return Ok(return_string.into_bytes());
        }
    }

    Err(-1)
}

/*
fn my_run_old(arguments: Vec<u8>) -> LocalResult<Vec<u8>> {
    // decoding arguments
    let args = String::from_utf8_lossy(&arguments[..]);
    let number = args.parse().map_err(|_| 1)?;

    let result = abi::double_host(number);
    // abi::sleep_secs(20);

    // serializing result
    let res_str = format!("{}", result);
    Ok(res_str.into_bytes())
}
*/
