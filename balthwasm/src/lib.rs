extern "C" {
    pub fn double_host(val: i32) -> i32;
    pub fn sleep_secs(val: u64);
}

#[no_mangle]
pub fn get_six() -> i32 {
    3 + 3
}

#[no_mangle]
pub fn double(val: i32) -> i32 {
    val * 2
}

#[no_mangle]
pub fn double_from_host(val: i32) -> i32 {
    unsafe { double_host(val) }
}

#[no_mangle]
pub fn run(val: i32) -> i32 {
    unsafe { sleep_secs(3) };
    double_from_host(val)
}

/*
#[no_mangle]
pub fn play_with_string(val: &str) -> String {
    format!("test: {}", val)
}

#[no_mangle]
pub fn play_with_array(val: &[u8]) -> Vec<u8> {
    let mut vec = Vec::new();
    vec.extend_from_slice(val);

    vec
}
*/
