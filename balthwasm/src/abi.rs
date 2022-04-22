#![allow(dead_code)]
use super::{my_run, my_test};

const BUFFER_CAPACITY: usize = 1024;

type WasmResult = i64;
pub type LocalResult<T> = Result<T, WasmResult>;

const RESULT_OK: WasmResult = 0;
pub const RESULT_ERROR: WasmResult = -1;

fn wasm_to_result(res: WasmResult) -> LocalResult<WasmResult> {
    if res < 0 {
        Err(res)
    } else {
        Ok(res)
    }
}

fn result_to_wasm(res: LocalResult<WasmResult>) -> WasmResult {
    match res {
        Ok(res) if res >= 0 => res,
        Err(res) if res < 0 => res,
        _ => RESULT_ERROR,
    }
}

fn get_argument_len() -> u32 {
    extern "C" {
        fn host_get_argument_len() -> u32;
    }

    unsafe { host_get_argument_len() }
}

fn get_argument() -> LocalResult<Vec<u8>> {
    extern "C" {
        fn host_get_argument(ptr: *const u8, len: u32) -> WasmResult;
    }

    let mut buffer = Vec::with_capacity(get_argument_len() as usize);
    let written_res = unsafe { host_get_argument(buffer.as_ptr(), buffer.capacity() as u32) };

    wasm_to_result(written_res).map(|o| {
        unsafe { buffer.set_len(o as usize) }
        buffer
    })
}

pub fn get_results_len() -> LocalResult<i64> {
    extern "C" {
        fn host_get_results_len() -> WasmResult;
    }

    wasm_to_result(unsafe { host_get_results_len() })
}

pub fn get_result_len(index: u32) -> LocalResult<i64> {
    extern "C" {
        fn host_get_result_len(index: u32) -> WasmResult;
    }

    wasm_to_result(unsafe { host_get_result_len(index) })
}

pub fn get_result(index: u32) -> LocalResult<Vec<u8>> {
    extern "C" {
        fn host_get_result(index: u32, ptr: *const u8, len: u32) -> WasmResult;
    }

    let mut buffer = Vec::with_capacity(get_result_len(index)? as usize);
    let written_res = unsafe { host_get_result(index, buffer.as_ptr(), buffer.capacity() as u32) };

    wasm_to_result(written_res).map(|o| {
        unsafe { buffer.set_len(o as usize) }
        buffer
    })
}

pub fn get_results() -> LocalResult<Vec<Vec<u8>>> {
    let results_len = get_results_len()?;
    let mut results = Vec::with_capacity(results_len as usize);

    for i in 0..results_len {
        results.push(get_result(i as u32)?)
    }

    Ok(results)
}

// TODO: results iterator to lazily get all results

fn send_result(res: &[u8]) -> LocalResult<()> {
    extern "C" {
        fn host_send_result(ptr: *const u8, len: usize) -> WasmResult;
    }

    wasm_to_result(unsafe { host_send_result(res.as_ptr(), res.len()) }).map(|_| ())
}

#[no_mangle]
pub fn run() -> WasmResult {
    if let Ok(argument_bytes) = get_argument() {
        if let Ok(result) = my_run(argument_bytes) {
            if send_result(&result[..]).is_ok() {
                return RESULT_OK;
            }
        }
    }
    RESULT_ERROR
}

#[no_mangle]
pub fn test() -> WasmResult {
    if let Ok(argument_bytes) = get_argument() {
        // We don't pass results directly in case only the first one is needed.
        return result_to_wasm(my_test(argument_bytes));
    }
    RESULT_ERROR
}

pub fn mark(val: i64) {
    extern "C" {
        pub fn mark(val: i64);
    }
    unsafe { mark(val) }
}

pub fn sleep_secs(secs: u64) {
    extern "C" {
        pub fn sleep_secs(val: u64);
    }
    unsafe { sleep_secs(secs) }
}

pub fn double_host(val: i32) -> i32 {
    extern "C" {
        pub fn double_host(val: i32) -> i32;
    }
    unsafe { double_host(val) }
}
