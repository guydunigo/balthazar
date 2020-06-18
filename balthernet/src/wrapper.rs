//! Provides [`BalthBehavioursWrapper`] to use several
//! [`NetworkBehaviour`](`libp2p::swarm::NetworkBehaviour`) at the same time.
use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    SinkExt, Stream,
};
use libp2p::{
    gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, Topic},
    // identify::{Identify, IdentifyEvent},
    identity::PublicKey,
    kad::{
        record::{store::MemoryStore, Key},
        Kademlia, KademliaEvent, QueryResult,
    },
    mdns::{Mdns, MdnsEvent},
    ping::{Ping, PingEvent},
    swarm::{
        protocols_handler::{IntoProtocolsHandler, ProtocolsHandler},
        NetworkBehaviour, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters,
    },
    NetworkBehaviour,
};
use misc::WorkerSpecs;
use proto::{manager, NodeTypeContainer, Message};
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use super::{
    balthazar::{self, BalthBehaviour},
    ManagerConfig, WorkerConfig,
};

const CHANNEL_SIZE: usize = 1024;

// TODO: better name...
const PUBSUB_TOPIC: &str = "balthazar";
fn get_topic() -> Topic {
    Topic::new(String::from(PUBSUB_TOPIC))
}
fn get_kad_key() -> Key {
    Key::new(&PUBSUB_TOPIC)
}

/// Events internal to [`BalthBehavioursWrapper`], hidden to outside Balthernet.
#[derive(Debug)]
enum EventIn {
    BalthBehaviour(balthazar::EventIn),
    ManagerMulticast(manager::ManagerMsgWrapper),
}

// TODO: better way to communicate with it ?
/// Handle to communicate towards the networking part.
#[derive(Clone, Debug)]
pub struct InputHandle {
    tx: Sender<EventIn>,
}

impl InputHandle {
    pub async fn send_to_managers(&mut self, msg: manager::ManagerMsgWrapper) {
        if let Err(e) = self.tx.send(EventIn::ManagerMulticast(msg)).await {
            panic!(
                "Balthernet input channel error while sending message to managers: {:?}",
                e
            );
        }
    }

    pub async fn send_to_behaviour(&mut self, event: balthazar::EventIn) {
        if let Err(e) = self.tx.send(EventIn::BalthBehaviour(event)).await {
            panic!(
                "Balthernet input channel error while sending event to behaviour: {:?}",
                e
            );
        }
    }
}

/// Use several [`NetworkBehaviour`](`libp2p::swarm::NetworkBehaviour`) at the same time.
#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll")]
#[behaviour(out_event = "balthazar::EventOut")]
pub struct BalthBehavioursWrapper {
    balthbehaviour: BalthBehaviour,
    mdns: Mdns,
    ping: Ping,
    kademlia: Kademlia<MemoryStore>, // TODO: look at MemoryStore
    // identify: Identify,
    gossipsub: Gossipsub,
    #[behaviour(ignore)]
    events: VecDeque<balthazar::EventOut>,
    #[behaviour(ignore)]
    inbound_rx: Receiver<EventIn>,
    #[behaviour(ignore)]
    managers_topic: Topic,
}

impl BalthBehavioursWrapper {
    /// Creates a new [`BalthBehavioursWrapper`] and returns a [`Sender`] channel
    /// to communicate with it from the exterior of the Swarm.
    pub fn new(
        node_type_conf: NodeTypeContainer<ManagerConfig, (WorkerConfig, WorkerSpecs)>,
        manager_check_interval: Duration,
        manager_timeout: Duration,
        pub_key: PublicKey,
    ) -> (Self, InputHandle) {
        let (tx, inbound_rx) = channel(CHANNEL_SIZE);
        let managers_topic = get_topic();

        let local_peer_id = pub_key.into_peer_id();
        let store = MemoryStore::new(local_peer_id.clone());
        // TODO: only for manager ? maybe use it also to find workers?
        let mut kademlia = Kademlia::new(local_peer_id.clone(), store);

        // TODO: only for manager ?
        // TODO: check message before propagating
        let mut gossipsub = Gossipsub::new(local_peer_id, GossipsubConfig::default());
        if let NodeTypeContainer::Manager(_) = node_type_conf {
            let success = gossipsub.subscribe(managers_topic.clone());
            if !success {
                unreachable!("Gossipsub couldn't subscribe, but we supposedly aren't already.");
            }
            // TODO: use Kademlia `get_providers` to find other managers
            // TODO: track queryids and no unwrap...
            kademlia.start_providing(get_kad_key()).unwrap();
        }

        let balthbehaviour =
            BalthBehaviour::new(node_type_conf, manager_check_interval, manager_timeout);

        (
            BalthBehavioursWrapper {
                balthbehaviour,
                mdns: Mdns::new().expect("Couldn't create a mDNS NetworkBehaviour"),
                ping: Ping::default(),
                kademlia,
                // TODO: better versions
                // identify: Identify::new("1.0".to_string(), "3.0".to_string(), pub_key),
                gossipsub,
                events: Default::default(),
                inbound_rx,
                managers_topic,
            },
            InputHandle { tx },
        )
    }

    fn poll(
        &mut self,
        cx: &mut Context,
        _params: &mut impl PollParameters,
        ) -> Poll<NetworkBehaviourAction<<<<Self as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent, <Self as NetworkBehaviour>::OutEvent>>
    // ) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, <Self as NetworkBehaviour>::OutEvent>>
    {
        if let Some(e) = self.events.pop_back() {
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(e))
        } else {
            // Reads the inbound channel to handle events:
            while let Poll::Ready(event_opt) = Stream::poll_next(Pin::new(&mut self.inbound_rx), cx)
            {
                match event_opt {
                    Some(EventIn::BalthBehaviour(event)) => {
                        // TODO: return directly here the result?
                        self.balthbehaviour.handle_event_in(event);
                    }
                    Some(EventIn::ManagerMulticast(msg)) => {
                        let mut buf = Vec::new();
                        msg.encode_length_delimited(&mut buf).expect("Could not encode manager message, buffer is a Vec and should have sufficient capacity.");
                        self.gossipsub.publish(&self.managers_topic, buf)
                    },
                    None => self.balthbehaviour.handle_event_in(balthazar::EventIn::Bye),
                }
                /*
                if action.is_ready() {
                    return action;
                }
                */
            }
            Poll::Pending
        }
    }
}

impl NetworkBehaviourEventProcess<balthazar::EventOut> for BalthBehavioursWrapper {
    fn inject_event(&mut self, event: balthazar::EventOut) {
        self.events.push_front(event);
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for BalthBehavioursWrapper {
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

impl NetworkBehaviourEventProcess<KademliaEvent> for BalthBehavioursWrapper {
    // Called when `kademlia` produces an event.
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            /*
            // TODO: not really useful now?
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
            */
            KademliaEvent::QueryResult {
                result: QueryResult::StartProviding(r),
                ..
            } => {
                eprintln!("Start providing result: {:?}", r);
            }
            _ => eprintln!("Kademlia: {:?}", message),
        }
    }
}

impl NetworkBehaviourEventProcess<PingEvent> for BalthBehavioursWrapper {
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
impl NetworkBehaviourEventProcess<IdentifyEvent> for BalthBehavioursWrapper {
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

impl NetworkBehaviourEventProcess<GossipsubEvent> for BalthBehavioursWrapper {
    fn inject_event(&mut self, event: GossipsubEvent) {
        if let GossipsubEvent::Message(peer_id, msg_id, msg) = event {
            println!(
                "Pubsub: message `{:?}` from `{:?}`: {:?}",
                peer_id, msg_id, msg
            );
        }
        /*
        match event {
            GossipsubEvent::Message(peer_id, msg_id, msg) => {
                println!(
                    "Pubsub: message `{:?}` from `{:?}`: {:?}",
                    peer_id, msg_id, msg
                );
            }
            GossipsubEvent::Subscribed { peer_id, topic } => println!(
                "Pubsub: Peer `{:?}` subscribed to topic `{:?}`",
                peer_id, topic
            ),
            GossipsubEvent::Unsubscribed { peer_id, topic } => println!(
                "Pubsub: Peer `{:?}` unsubscribed to topic `{:?}`",
                peer_id, topic
            ),
        }
        */
    }
}
