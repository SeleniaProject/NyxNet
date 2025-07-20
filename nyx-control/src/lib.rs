#![forbid(unsafe_code)]

#[cfg(feature = "dht")]
use futures::StreamExt;
#[cfg(feature = "dht")]
use libp2p::{identity, kad::{store::MemoryStore, Kademlia, Quorum, record::{Key, Record}}, swarm::SwarmEvent, PeerId, Multiaddr};
use tokio::sync::mpsc;

pub mod settings;
pub mod probe;

/// Control messages for the DHT event loop.
#[cfg(feature = "dht")]
pub enum DhtCmd {
    Put { key: String, value: Vec<u8> },
    Get { key: String, resp: mpsc::Sender<Option<Vec<u8>>> },
    Bootstrap(Multiaddr),
}

#[cfg(not(feature = "dht"))]
pub enum DhtCmd { Stub }

/// Handle to interact with running DHT node.
#[derive(Clone)]
pub struct DhtHandle {
    #[cfg(feature = "dht")]
    tx: mpsc::Sender<DhtCmd>,
}

impl DhtHandle {
    #[cfg(feature = "dht")]
    pub async fn put(&self, key: &str, val: Vec<u8>) {
        let _ = self.tx.send(DhtCmd::Put { key: key.to_string(), value: val }).await;
    }

    #[cfg(feature = "dht")]
    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        let _ = self.tx.send(DhtCmd::Get { key: key.to_string(), resp: resp_tx }).await;
        resp_rx.recv().await.flatten()
    }

    #[cfg(feature = "dht")]
    pub async fn add_bootstrap(&self, addr: Multiaddr) {
        let _ = self.tx.send(DhtCmd::Bootstrap(addr)).await;
    }

    #[cfg(not(feature = "dht"))]
    pub async fn put(&self, _key: &str, _val: Vec<u8>) {}

    #[cfg(not(feature = "dht"))]
    pub async fn get(&self, _key: &str) -> Option<Vec<u8>> { None }

    #[cfg(not(feature = "dht"))]
    pub async fn add_bootstrap(&self, _addr: ()) {}
}

/// Spawn DHT node; returns handle to interact.
#[cfg(feature = "dht")]
pub async fn spawn_dht() -> DhtHandle {
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());

    // Build TCP+Noise+Yamux transport manually (tokio backend)
    let transport = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default())
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(libp2p::noise::NoiseAuthenticated::xx(&id_keys).unwrap())
        .multiplex(libp2p::yamux::YamuxConfig::default())
        .boxed();

    let store = MemoryStore::new(peer_id);
    let behaviour = Kademlia::new(peer_id, store);

    let mut swarm = libp2p::swarm::SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id)
        .build();

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

// Fallback stub when the `dht` feature is disabled.
#[cfg(not(feature = "dht"))]
pub async fn spawn_dht() -> DhtHandle {
    let (_tx, _rx) = mpsc::channel::<DhtCmd>(1);
    DhtHandle {}
}
