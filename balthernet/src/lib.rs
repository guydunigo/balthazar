//! This crate handles the peer-to-peer networking part, currently with [`libp2p`].
//!
//! ## Procedure when adding events or new messages
//!
//! See the documentation of module [`balthazar`].
//!
//! ## Set up instructions
//!
//! TODO: base instructions to set it up.
extern crate balthamisc as misc;
extern crate balthaproto as proto;
extern crate futures;
extern crate libp2p;

pub mod balthazar;
pub mod tcp_transport;
mod wrapper;

pub use wrapper::BalthBehavioursWrapper;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
