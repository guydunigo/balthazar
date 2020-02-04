//! This crate handles the peer-to-peer networking part, currently with [`libp2p`].
//!
//! When adding message types and events, go see the documentation of module [`balthazar`].
//!
//! TODO: base instructions to set it up.
extern crate balthamisc as misc;
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
