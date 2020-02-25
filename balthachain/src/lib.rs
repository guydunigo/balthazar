extern crate futures;
extern crate web3;

mod config;
pub use config::ChainConfig;

use futures::{compat::Compat01As03, executor::block_on};

pub fn run() {
    let (_eloop, transport) = web3::transports::Http::new("http://localhost:8545").unwrap();

    let web3 = web3::Web3::new(transport);
    let accounts_futures = Compat01As03::new(web3.eth().accounts());
    let accounts = block_on(accounts_futures).unwrap();

    println!("Accounts: {:?}", accounts);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
