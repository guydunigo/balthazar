#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;
extern crate sha3;

use ron::ser;

use sha3::{Digest, Sha3_256};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

extern "C" {
    fn push_byte(s: u8);
    fn get_byte() -> u8;
    fn get_bytes_len() -> u64;
    fn tcp_listen_init(port: u16) -> i8;
    fn tcp_accept(listener_id: i8) -> i8;
    fn tcp_get_socket_addr_is_v6(sock_id: i8) -> i8;
    fn tcp_get_socket_addr_nth(sock_id: i8, nth: u8) -> i16;
    fn tcp_read_byte(sock_id: i8) -> i16;
    fn tcp_write_byte(sock_id: i8, byte: u8) -> i16;
    fn tcp_close(sock_id: i8) -> i8;
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

    unsafe {
        let lsock_id = tcp_listen_init(2000);
        if lsock_id < 0 {
            // TODO: error
        }

        loop {
            // TODO: test for negative values
            let csock_id = tcp_accept(lsock_id);
            let is_ipv6 = tcp_get_socket_addr_is_v6(csock_id) == 1;
            let addr = if is_ipv6 {
                let mut array: [u8; 16] = [0; 16];
                for i in 0..16 {
                    array[i] = tcp_get_socket_addr_nth(csock_id, i as u8) as u8;
                }
                IpAddr::V6(Ipv6Addr::from(array))
            } else {
                let mut array: [u8; 4] = [0; 4];
                for i in 0..4 {
                    array[i] = tcp_get_socket_addr_nth(csock_id, i as u8) as u8;
                }
                IpAddr::V4(Ipv4Addr::from(array))
            };

            let str_addr = format!("{}", addr);
            str_addr
                .into_bytes()
                .iter()
                .skip_while(|b| tcp_write_byte(csock_id, **b) >= 0)
                .next();

            tcp_close(csock_id);
        }
    }
}
