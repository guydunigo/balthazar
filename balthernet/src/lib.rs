extern crate balthalib as lib;
extern crate balthaproto as proto;
extern crate futures;
extern crate libp2p;

pub mod balthazar;
pub mod wrapper;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
