//! Provides [`BalthBehavioursWrapper`] to use several
//! [`NetworkBehaviour`](`libp2p::swarm::NetworkBehaviour`) at the same time.
use futures::io::{AsyncRead, AsyncWrite};
use libp2p::{
    // identify::{Identify, IdentifyEvent},
    identity::PublicKey,
    kad::{record::store::MemoryStore, Kademlia, KademliaEvent, PutRecordOk, Record},
    mdns::{Mdns, MdnsEvent},
    ping::{Ping, PingEvent},
    swarm::{
        protocols_handler::{IntoProtocolsHandler, ProtocolsHandler},
        NetworkBehaviour, NetworkBehaviourAction, NetworkBehaviourEventProcess,
    },
    NetworkBehaviour,
};
use std::{
    collections::VecDeque,
    task::{Context, Poll},
};
use tokio::sync::mpsc::Sender;

use super::balthazar::{self, BalthBehaviour};
use misc::NodeType;

/// Use several [`NetworkBehaviour`](`libp2p::swarm::NetworkBehaviour`) at the same time.
#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll")]
#[behaviour(out_event = "balthazar::EventOut")]
pub struct BalthBehavioursWrapper<TSubstream>
where
    TSubstream: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    balthbehaviour: BalthBehaviour<TSubstream>,
    mdns: Mdns<TSubstream>,
    ping: Ping<TSubstream>,
    kademlia: Kademlia<TSubstream, MemoryStore>,
    // identify: Identify<TSubstream>,
    #[behaviour(ignore)]
    events: VecDeque<balthazar::EventOut>,
}

impl<T> BalthBehavioursWrapper<T>
where
    T: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    /// Creates a new [`BalthBehavioursWrapper`] and returns a [`Sender`] channel
    /// to communicate with it from the exterior of the Swarm.
    pub fn new(node_type: NodeType<()>, pub_key: PublicKey) -> (Self, Sender<balthazar::EventIn>) {
        let local_peer_id = pub_key.into_peer_id();
        let store = MemoryStore::new(local_peer_id.clone());
        let (balthbehaviour, tx) = BalthBehaviour::new(node_type);

        (
            BalthBehavioursWrapper {
                balthbehaviour,
                mdns: Mdns::new().expect("Couldn't create a Mdns NetworkBehaviour"),
                ping: Ping::default(),
                kademlia: Kademlia::new(local_peer_id, store),
                // identify: Identify::new("1.0".to_string(), "3.0".to_string(), pub_key),
                events: Default::default(),
            },
            tx,
        )
    }

fn poll(&mut self, _cx: &mut Context) -> Poll<NetworkBehaviourAction<<<<Self as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent, <Self as NetworkBehaviour>::OutEvent>>{
        if let Some(e) = self.events.pop_back() {
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(e))
        } else {
            Poll::Pending
        }
    }
}

impl<T> NetworkBehaviourEventProcess<balthazar::EventOut> for BalthBehavioursWrapper<T>
where
    T: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    fn inject_event(&mut self, event: balthazar::EventOut) {
        self.events.push_front(event);
    }
}

impl<T> NetworkBehaviourEventProcess<MdnsEvent> for BalthBehavioursWrapper<T>
where
    T: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event {
            for (peer_id, multiaddr) in list {
                // println!("Mdns discovered: {:?} {:?}", peer_id, multiaddr);
                self.kademlia.add_address(&peer_id, multiaddr.clone());
                self.balthbehaviour.inject_mdns_event(peer_id, multiaddr);
            }
        }
    }
}

impl<T> NetworkBehaviourEventProcess<KademliaEvent> for BalthBehavioursWrapper<T>
where
    T: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    // Called when `kademlia` produces an event.
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            KademliaEvent::GetRecordResult(Ok(result)) => {
                for Record { key, value, .. } in result.records {
                    println!(
                        "Got record {:?} {:?}",
                        std::str::from_utf8(key.as_ref()).unwrap(),
                        std::str::from_utf8(&value).unwrap(),
                    );
                }
            }
            KademliaEvent::GetRecordResult(Err(err)) => {
                eprintln!("Failed to get record: {:?}", err);
            }
            KademliaEvent::PutRecordResult(Ok(PutRecordOk { key })) => {
                println!(
                    "Successfully put record {:?}",
                    std::str::from_utf8(key.as_ref()).unwrap()
                );
            }
            KademliaEvent::PutRecordResult(Err(err)) => {
                eprintln!("Failed to put record: {:?}", err);
            }
            _ => {}
        }
    }
}

impl<T> NetworkBehaviourEventProcess<PingEvent> for BalthBehavioursWrapper<T>
where
    T: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    fn inject_event(&mut self, _event: PingEvent) {
        /*
        match event.result {
            Ok(s) => println!("{:?} : peer success : {:?}", event.peer, s),
            Err(e) => println!("{:?} : peer error : {:?}", event.peer, e),
        }
        */
    }
}

/*
impl<T> NetworkBehaviourEventProcess<IdentifyEvent> for BalthBehavioursWrapper<T>
where
    T: 'static + Send + Sync + Unpin + AsyncRead + AsyncWrite,
{
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: IdentifyEvent) {
        if let IdentifyEvent::Received {
            peer_id,
            info,
            observed_addr,
        } = event
        {
            println!("{:?} {:?} {:?}", peer_id, info, observed_addr);
        } else {
            println!("Identity other");
        }
    }
}
*/
