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
extern crate void;

use futures::Stream;
use libp2p::build_development_transport;
// use libp2p::build_tcp_ws_secio_mplex_yamux;
/// To avoid importing the whole libp2p crate in another one...
pub use libp2p::{identity, Multiaddr};
use libp2p::{identity::Keypair, swarm::Swarm};
use misc::NodeType;

pub mod balthazar;
pub mod tcp_transport;
mod wrapper;
pub use wrapper::BalthBehavioursWrapper;

// TODO: better control over Swarm object and solve return type hell: use of channel ?
// TODO: NodeType containing manager to try ?
/// Creates a new swarm based on [`BalthBehaviour`](`balthazar::BalthBehaviour`) and a default transport and returns
/// a stream of event coming out of [`BalthBehaviour`](`balthazar::BalthBehaviour`).
pub fn get_swarm(
    node_type: NodeType<()>,
    keypair: Keypair,
    listen_addr: Multiaddr,
    addresses_to_dial: &[Multiaddr],
) -> impl Stream<Item = balthazar::BalthBehaviourEvent> {
    let keypair_public = keypair.public();
    let peer_id = keypair_public.clone().into_peer_id();
    // TODO: retry that
    let net_behaviour = BalthBehavioursWrapper::new(node_type, keypair.public());
    // let net_behaviour = balthazar::BalthBehaviour::new(node_type);

    // TODO: inspect the two build things and errors
    // let transport = tcp_transport::get_tcp_transport(keypair);
    let transport = build_development_transport(keypair).unwrap();
    // let transport = build_tcp_ws_secio_mplex_yamux(keypair).unwrap();

    let mut swarm = Swarm::new(transport, net_behaviour, peer_id);

    Swarm::listen_on(&mut swarm, listen_addr).unwrap();

    addresses_to_dial.iter().for_each(|addr| {
        Swarm::dial_addr(&mut swarm, addr.clone()).unwrap();
        println!("Dialed {:?}", addr);
    });

    for addr in Swarm::listeners(&swarm) {
        println!("Listening on {}", addr);
    }

    swarm
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
