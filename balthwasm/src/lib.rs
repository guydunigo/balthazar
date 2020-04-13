extern crate multihash;

use multihash::Keccak256;

mod abi;
use abi::LocalResult;

// ------------------------------------------------------------------
//                          User's code
// ------------------------------------------------------------------

pub fn my_run(argument: Vec<u8>) -> LocalResult<Vec<u8>> {
    // TODO: checking arguments.
    for counter in 0.. {
        let hash = hash_counter(counter);
        if contains_pattern(&argument[..], &hash[..]) {
            let return_string = format!("({},{})", counter, hash);
            return Ok(return_string.into_bytes());
        }
    }
    Err(abi::RESULT_ERROR)
}

pub fn my_test(argument: Vec<u8>) -> LocalResult<i64> {
    for i in 0..abi::get_results_len()? {
        let result = abi::get_result(i as u32)?;
        if test_result(&argument[..], result)?.is_some() {
            return Ok(i);
        }
    }
    Err(abi::RESULT_ERROR)
}

fn hash_counter(counter: usize) -> String {
    let string = format!("{}", counter);
    let hash_bytes = Keccak256::digest(string.as_bytes()).into_bytes();
    hash_bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn contains_pattern(pattern: &[u8], txt: &str) -> bool {
    txt.as_bytes()
        .windows(pattern.len())
        .any(|w| *w == pattern[..])
}

pub fn test_result(argument: &[u8], result: Vec<u8>) -> LocalResult<Option<()>> {
    let result = String::from_utf8_lossy(&result[1..result.len() - 1]);
    let mut elems_iter = result.split(',');
    let counter: usize = if let Some(counter) = elems_iter.next() {
        counter.parse().map_err(|_| abi::RESULT_ERROR)?
    } else {
        return Err(abi::RESULT_ERROR);
    };
    let hash = elems_iter.next().ok_or(abi::RESULT_ERROR)?;

    if hash_counter(counter) == hash && contains_pattern(argument, hash) {
        Ok(Some(()))
    } else {
        Ok(None)
    }
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
