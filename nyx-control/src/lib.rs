#![forbid(unsafe_code)]

use std::{time::Duration, sync::Arc};
use futures::{StreamExt};
use libp2p::{identity, kad::{Kademlia, store::MemoryStore, record::{Key, Record}, Quorum}, swarm::SwarmEvent, PeerId, Multiaddr, Transport};
use tokio::sync::mpsc;

pub mod settings;

/// Control messages for the DHT event loop.
pub enum DhtCmd {
    Put { key: String, value: Vec<u8> },
    Get { key: String, resp: mpsc::Sender<Option<Vec<u8>>> },
    Bootstrap(Multiaddr),
}

/// Handle to interact with running DHT node.
#[derive(Clone)]
pub struct DhtHandle {
    tx: mpsc::Sender<DhtCmd>,
}

impl DhtHandle {
    pub async fn put(&self, key: &str, val: Vec<u8>) {
        let _ = self.tx.send(DhtCmd::Put { key: key.to_string(), value: val }).await;
    }

    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let _ = self.tx.send(DhtCmd::Get { key: key.to_string(), resp: resp_tx }).await;
        resp_rx.recv().await.flatten()
    }

    pub async fn add_bootstrap(&self, addr: Multiaddr) {
        let _ = self.tx.send(DhtCmd::Bootstrap(addr)).await;
    }
}

/// Spawn DHT node; returns handle to interact.
pub async fn spawn_dht() -> DhtHandle {
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());

    let transport = libp2p::tokio_development_transport(id_keys).await.unwrap();
    let store = MemoryStore::new(peer_id);
    let mut behaviour = Kademlia::new(peer_id, store);
    behaviour.set_query_timeout(Duration::from_secs(10));

    let mut swarm = libp2p::Swarm::with_tokio_executor(transport, behaviour, peer_id);

    let (tx, mut rx) = mpsc::channel::<DhtCmd>(32);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => match cmd {
                    DhtCmd::Put { key, value } => {
                        let _ = swarm.behaviour_mut().put_record(Record::new(Key::new(&key), value), Quorum::One);
                    }
                    DhtCmd::Get { key, resp } => {
                        let _ = swarm.behaviour_mut().get_record(Key::new(&key), Quorum::One);
                        // Response will be sent via KademliaEvent below.
                        // Store sender in behaviour not implemented for brevity.
                        drop(resp);
                    }
                    DhtCmd::Bootstrap(addr) => {
                        let _ = swarm.dial(addr);
                    }
                },
                event = swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(libp2p::kad::KademliaEvent::OutboundQueryCompleted { .. }) => {}
                    _ => {}
                }
            }
        }
    });

    DhtHandle { tx }
}
