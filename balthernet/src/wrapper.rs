//! Provides [`BalthBehavioursWrapper`] to use several
//! [`NetworkBehaviour`](`libp2p::swarm::NetworkBehaviour`) at the same time.
use futures::io::{AsyncRead, AsyncWrite};
use libp2p::{
    identify::{Identify, IdentifyEvent},
    identity::PublicKey,
    kad::{record::store::MemoryStore, Kademlia, KademliaEvent, PutRecordOk, Record},
    mdns::{Mdns, MdnsEvent},
    ping::{Ping, PingEvent},
    swarm::NetworkBehaviourEventProcess,
    NetworkBehaviour,
};

use super::balthazar::{BalthBehaviour, BalthBehaviourEvent, BalthBehaviourKindOfEvent};
use misc::NodeType;

/// Use several [`NetworkBehaviour`](`libp2p::swarm::NetworkBehaviour`) at the same time.
#[derive(NetworkBehaviour)]
pub struct BalthBehavioursWrapper<TSubstream> {
    balthbehaviour: BalthBehaviour<TSubstream>,
    ping: Ping<TSubstream>,
    mdns: Mdns<TSubstream>,
    kademlia: Kademlia<TSubstream, MemoryStore>,
    identify: Identify<TSubstream>,
}

impl<T> BalthBehavioursWrapper<T> {
    pub fn new(node_type: NodeType<()>, pub_key: PublicKey) -> Self {
        let local_peer_id = pub_key.clone().into_peer_id();
        let store = MemoryStore::new(local_peer_id.clone());
        BalthBehavioursWrapper {
            balthbehaviour: BalthBehaviour::new(node_type),
            ping: Ping::default(),
            mdns: Mdns::new().expect("Couldn't create a Mdns NetworkBehaviour"),
            kademlia: Kademlia::new(local_peer_id, store),
            identify: Identify::new("1.0".to_string(), "3.0".to_string(), pub_key),
        }
    }
}

impl<T> NetworkBehaviourEventProcess<BalthBehaviourEvent> for BalthBehavioursWrapper<T>
where
    T: AsyncRead + AsyncWrite,
{
    fn inject_event(&mut self, event: BalthBehaviourEvent) {
        eprintln!("Event from BalthBehaviourEvent for {:?}", event.peer_id());
    }
}

impl<T> NetworkBehaviourEventProcess<MdnsEvent> for BalthBehavioursWrapper<T>
where
    T: AsyncRead + AsyncWrite,
{
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event {
            for (peer_id, multiaddr) in list {
                // println!("Mdns discovered: {:?} {:?}", peer_id, multiaddr);
                self.kademlia.add_address(&peer_id, multiaddr.clone());
                self.balthbehaviour
                    .events_mut()
                    .push_front(BalthBehaviourEvent::new(
                        peer_id,
                        BalthBehaviourKindOfEvent::Mdns(multiaddr),
                    ));
            }
        }
    }
}

impl<T> NetworkBehaviourEventProcess<KademliaEvent> for BalthBehavioursWrapper<T>
where
    T: AsyncRead + AsyncWrite,
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

impl<T> NetworkBehaviourEventProcess<IdentifyEvent> for BalthBehavioursWrapper<T>
where
    T: AsyncRead + AsyncWrite,
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

impl<T> NetworkBehaviourEventProcess<PingEvent> for BalthBehavioursWrapper<T>
where
    T: AsyncRead + AsyncWrite,
{
    fn inject_event(&mut self, event: PingEvent) {
        match event.result {
            Ok(s) => println!("{:?} : peer success : {:?}", event.peer, s),
            Err(e) => println!("{:?} : peer error : {:?}", event.peer, e),
        }
    }
}
