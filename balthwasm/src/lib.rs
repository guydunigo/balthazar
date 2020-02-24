mod abi;
use abi::LocalResult;

// ------------------------------------------------------------------
//                          User's code
// ------------------------------------------------------------------

fn my_run(arguments: Vec<u8>) -> LocalResult<Vec<u8>> {
    // decoding arguments
    let args = String::from_utf8_lossy(&arguments[..]);
    let number = args.parse().map_err(|_| 1)?;

    let result = abi::double_host(number);

    // serializing result
    let res_str = format!("{}", result);
    Ok(res_str.into_bytes())
}
