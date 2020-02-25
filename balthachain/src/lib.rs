extern crate futures;
extern crate web3;

mod config;
pub use config::{Address, ChainConfig};
use web3::transports::{EventLoopHandle, Http};

use futures::{compat::Compat01As03, executor::block_on};

pub fn run(config: &ChainConfig) {
    let (_eloop, web3) = get_client(config.web3_http());
    let accounts_futures = Compat01As03::new(web3.eth().accounts());
    let accounts = block_on(accounts_futures).unwrap();

    println!("Accounts: {:?}", accounts);
}

pub fn get_client(web3_http: &str) -> (EventLoopHandle, web3::Web3<Http>) {
    let (eloop, transport) = web3::transports::Http::new(web3_http).unwrap();
    (eloop, web3::Web3::new(transport))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
