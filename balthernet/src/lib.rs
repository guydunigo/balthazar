//! This crate handles the peer-to-peer networking part, currently with [`libp2p`].
//!
//! TODO: base instructions to set it up.
//! TODO: instructions to extend with new messages.
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
