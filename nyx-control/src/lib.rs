#![forbid(unsafe_code)]

#[cfg(feature = "dht")]
use futures::StreamExt;
#[cfg(feature = "dht")]
use libp2p::{identity, kad::{store::MemoryStore, Kademlia, Quorum, record::{Key, Record}}, swarm::SwarmEvent, PeerId, Multiaddr};
use tokio::sync::{mpsc, oneshot};
use std::collections::HashMap;
use nyx_core::{NyxConfig};

/// Aggregates control-plane handles (DHT + optional Push service).
#[derive(Clone)]
pub struct ControlManager {
    pub dht: DhtHandle,
    pub push: Option<PushHandle>,
}

/// Initialize control plane based on runtime configuration.
///
/// * Spawns Kademlia DHT node (feature gated)
/// * Starts background push service when `cfg.push` is provided
///
/// This async helper is intended to be invoked by the Nyx daemon at startup.
pub async fn init_control(cfg: &NyxConfig) -> ControlManager {
    let dht = spawn_dht().await;
    let push = cfg.push.clone().map(spawn_push_service);

    // If rendezvous endpoint configured in env NYX_RENDEZVOUS_URL
    if let Ok(url) = std::env::var("NYX_RENDEZVOUS_URL") {
        // Use node_id from config or fallback random.
        let bytes = cfg.node_id.as_ref()
            .and_then(|s| hex::decode(s).ok())
            .unwrap_or(vec![0u8;32]);
        let mut id = [0u8;32];
        id.copy_from_slice(&bytes[..32]);
        let client = rendezvous::RendezvousClient::new(url, id, dht.listen_addr().clone(), dht.clone());
        client.spawn();
    }
    ControlManager { dht, push }
}

pub mod settings;
pub mod probe;
pub mod push;
pub use push::{PushHandle, spawn_push_service};
pub mod rendezvous;
pub use rendezvous::RendezvousClient as RendezvousService;
mod settings_sync;

/// Control messages for the DHT event loop.
#[cfg(feature = "dht")]
pub enum DhtCmd {
    Put { key: String, value: Vec<u8> },
    Get { key: String, resp: oneshot::Sender<Option<Vec<u8>>> },
    Bootstrap(Multiaddr),
}

#[cfg(not(feature = "dht"))]
pub enum DhtCmd { Stub }

/// Handle to interact with running DHT node.
#[derive(Clone)]
pub struct DhtHandle {
    #[cfg(feature = "dht")]
    tx: mpsc::Sender<DhtCmd>,
    #[cfg(feature = "dht")]
    listen_addr: Multiaddr,
}

impl DhtHandle {
    #[cfg(feature = "dht")]
    pub async fn put(&self, key: &str, val: Vec<u8>) {
        let _ = self.tx.send(DhtCmd::Put { key: key.to_string(), value: val }).await;
    }

    #[cfg(feature = "dht")]
    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let _ = self.tx.send(DhtCmd::Get { key: key.to_string(), resp: resp_tx }).await;
        resp_rx.await.ok().flatten()
    }

    #[cfg(feature = "dht")]
    pub async fn add_bootstrap(&self, addr: Multiaddr) {
        let _ = self.tx.send(DhtCmd::Bootstrap(addr)).await;
    }

    // Return the primary listen address when DHT is enabled.
    #[cfg(feature = "dht")]
    #[must_use]
    pub fn listen_addr(&self) -> &libp2p::Multiaddr {
        &self.listen_addr
    }

    #[cfg(not(feature = "dht"))]
    #[must_use]
    pub fn listen_addr(&self) {}

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

    // Use convenience helper for typical TCP/Noise/Yamux transport (tokio backend)
    let transport = libp2p::tokio_development_transport(id_keys.clone()).unwrap();

    let store = MemoryStore::new(peer_id);
    let behaviour = Kademlia::new(peer_id, store);

    let mut swarm = libp2p::Swarm::new(transport, behaviour, peer_id);

    let (tx, mut rx) = mpsc::channel::<DhtCmd>(32);
    let mut pending_get: HashMap<libp2p::kad::QueryId, oneshot::Sender<Option<Vec<u8>>>> = HashMap::new();

    // Start listening before moving swarm into task.
    use libp2p::Multiaddr;
    let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
    swarm.listen_on(listen_addr.clone()).expect("listen");

    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => match cmd {
                    DhtCmd::Put { key, value } => {
                        let _ = swarm.behaviour_mut().put_record(Record::new(Key::new(&key), value), Quorum::One);
                    }
                    DhtCmd::Get { key, resp } => {
                        let query_id = swarm.behaviour_mut().get_record(Key::new(&key), Quorum::One);
                        pending_get.insert(query_id, resp);
                    }
                    DhtCmd::Bootstrap(addr) => {
                        let _ = swarm.dial(addr);
                    }
                },
                event = swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(libp2p::kad::KademliaEvent::OutboundQueryCompleted { id, result, .. }) => {
                        if let Some(sender) = pending_get.remove(&id) {
                            let value = match result {
                                libp2p::kad::QueryResult::GetRecord(Ok(r)) => {
                                    r.records.first().map(|rec| rec.record.value.clone())
                                },
                                _ => None,
                            };
                            let _ = sender.send(value);
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    DhtHandle { tx, listen_addr }
}

// Fallback stub when the `dht` feature is disabled.
#[cfg(not(feature = "dht"))]
pub async fn spawn_dht() -> DhtHandle {
    let (_tx, _rx) = mpsc::channel::<DhtCmd>(1);
    DhtHandle {}
}
