#![forbid(unsafe_code)]

use async_std::task;
use libp2p::{identity, kad::{Kademlia, store::MemoryStore, record::Key}, Multiaddr, Swarm, PeerId, kad::KademliaEvent};
use std::time::Duration;

pub struct DhtNode {
    swarm: Swarm<Kademlia<MemoryStore>>,
}

impl DhtNode {
    pub fn new() -> Self {
        let id_keys = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        let store = MemoryStore::new(peer_id);
        let kademlia = Kademlia::new(peer_id, store);
        let swarm = Swarm::with_async_std_executor(libp2p::development_transport(id_keys.clone()).unwrap(), kademlia, peer_id);
        Self { swarm }
    }

    pub fn start(&mut self) {
        // spawn drive
        task::spawn(async move {
            loop {
                self.swarm.next().await;
            }
        });
    }

    pub fn put_record(&mut self, key: &str, value: Vec<u8>) {
        let record_key = Key::new(&key);
        let _ = self.swarm.behaviour_mut().put_record(libp2p::kad::Record::new(record_key, value), libp2p::kad::Quorum::One);
    }
}
