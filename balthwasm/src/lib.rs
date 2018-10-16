#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;
extern crate sha3;

use ron::ser;

use sha3::{Digest, Sha3_256};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

extern "C" {
    fn debug(n: i8);
    fn push_byte(s: u8);
    fn get_byte() -> u8;
    fn get_bytes_len() -> u64;
    fn tcp_listen_init(port: u16) -> i8;
    fn tcp_accept(listener_id: i8) -> i8;
    fn tcp_socket_addr_is_v6(sock_id: i8) -> i8;
    fn tcp_socket_addr_nth(sock_id: i8, nth: u8) -> i16;
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
    unsafe {
        let lsock_id = tcp_listen_init(2000);
        debug(lsock_id);
        if lsock_id < 0 {
            // TODO: error
        }

        loop {
            // TODO: test for negative values
            let csock_id = tcp_accept(lsock_id);
            debug(csock_id);

            let is_ipv6 = tcp_socket_addr_is_v6(csock_id) == 1;
            let addr = if is_ipv6 {
                let mut array: [u8; 16] = [0; 16];
                for i in 0..16 {
                    array[i] = tcp_socket_addr_nth(csock_id, i as u8) as u8;
                }
                IpAddr::V6(Ipv6Addr::from(array))
            } else {
                let mut array: [u8; 4] = [0; 4];
                for i in 0..4 {
                    array[i] = tcp_socket_addr_nth(csock_id, i as u8) as u8;
                }
                IpAddr::V4(Ipv4Addr::from(array))
            };

            let str_addr = format!("{}", addr);

            let mut hasher = Sha3_256::default();
            hasher.input(&str_addr.into_bytes()[..]);

            let hash = hasher.result();
            let str_hash = format!("{:x}", hash);

            str_hash
                .into_bytes()
                .iter()
                .skip_while(|b| tcp_write_byte(csock_id, **b) >= 0)
                .next();

            tcp_close(csock_id);
        }
    }
}
