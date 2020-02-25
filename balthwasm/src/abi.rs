#![allow(dead_code)]
use super::my_run;

const BUFFER_CAPACITY: usize = 1024;

type WasmResult = i64;
pub type LocalResult<T> = Result<T, WasmResult>;

const RESULT_OK: WasmResult = 0;
const RESULT_ERROR: WasmResult = -1;

fn wasm_to_result(res: WasmResult) -> Result<WasmResult, WasmResult> {
    if res < 0 {
        Err(res)
    } else {
        Ok(res)
    }
}

fn get_arguments() -> LocalResult<Vec<u8>> {
    extern "C" {
        fn host_get_arguments(ptr: *const u8, len: u32) -> WasmResult;
    };

    let mut buffer = Vec::with_capacity(BUFFER_CAPACITY);
    let written_res = unsafe { host_get_arguments(buffer.as_ptr(), buffer.capacity() as u32) };

    wasm_to_result(written_res).map(|o| {
        unsafe { buffer.set_len(o as usize) };
        buffer
    })
}

fn get_result() -> LocalResult<Vec<u8>> {
    extern "C" {
        fn host_get_result(ptr: *const u8, len: u32) -> WasmResult;
    };

    let mut buffer = Vec::with_capacity(BUFFER_CAPACITY);
    let written_res = unsafe { host_get_result(buffer.as_ptr(), buffer.capacity() as u32) };

    wasm_to_result(written_res).map(|o| {
        unsafe { buffer.set_len(o as usize) };
        buffer
    })
}

fn send_result(res: &[u8]) -> LocalResult<()> {
    extern "C" {
        fn host_send_result(ptr: *const u8, len: usize) -> WasmResult;
    };

    wasm_to_result(unsafe { host_send_result(res.as_ptr(), res.len()) }).map(|_| ())
}

#[no_mangle]
pub fn run() -> WasmResult {
    if let Ok(arguments_bytes) = get_arguments() {
        if let Ok(result) = my_run(arguments_bytes) {
            if send_result(&result[..]).is_ok() {
                return RESULT_OK;
            }
        }
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
