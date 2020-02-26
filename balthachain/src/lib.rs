extern crate futures;
extern crate web3;

mod config;
pub use config::{Address, ChainConfig};
use web3::{
    contract::Contract,
    transports::{EventLoopHandle, Http},
    Web3,
};

use futures::{compat::Compat01As03, executor::block_on};

pub fn run(config: &ChainConfig) {
    let (_eloop, web3) = get_client(config.web3_http());

    /*
    let accounts_futures = Compat01As03::new(web3.eth().accounts());
    let accounts = block_on(accounts_futures).unwrap();
    */
    if let Some((job_addr, abi)) = config.contract_jobs() {
        let contract = Contract::from_json(web3.eth(), *job_addr, &abi[..]).unwrap();
        println!("Addr: {:?}", contract.address());
        if let Some(addr) = config.ethereum_address() {
            let fut = contract.call("set_counter", 0 as u64, *addr, Default::default());
            block_on(Compat01As03::new(fut)).unwrap();

            let fut = contract.call("counter", (), *addr, Default::default());
            println!("Counter: {:?}", block_on(Compat01As03::new(fut)).unwrap());

            let fut = contract.call("inc_counter", 0 as u64, *addr, Default::default());
            block_on(Compat01As03::new(fut)).unwrap();

            let fut = contract.call("counter", (), *addr, Default::default());
            println!("Counter: {:?}", block_on(Compat01As03::new(fut)).unwrap());
        }
    }
}

pub fn get_client(web3_http: &str) -> (EventLoopHandle, Web3<Http>) {
    let (eloop, transport) = Http::new(web3_http).unwrap();
    (eloop, web3::Web3::new(transport))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
