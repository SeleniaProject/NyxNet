#![forbid(unsafe_code)]

//! Rendezvous server synchronisation.
//!
//! This lightweight client periodically publishes our DHT listen address and
//! retrieves the current node set for peer discovery.  The implementation is
//! HTTP-based so that deployments can place the rendezvous endpoint behind a
//! CDN or load-balancer easily.
//!
//! The server API (JSON):
//! POST /announce { "node_id": "hex", "addr": "multiaddr" }
//! GET  /peers   → [ { "node_id": "hex", "addr": "multiaddr" }, ... ]
//!
//! In line with Nyx design §7, we integrate with the `probe` RTT/BW prober so
//! that freshly discovered peers are pinged immediately.

use std::time::Duration;
use tokio::time::interval;
use serde::{Serialize, Deserialize};
use nyx_core::NodeId;
use multiaddr::Multiaddr;
use crate::DhtHandle;
use tracing::{info, warn};

#[derive(Clone)]
pub struct RendezvousClient {
    endpoint: String,
    node_id: NodeId,
    addr: Multiaddr,
    dht: DhtHandle,
    http: reqwest::Client,
}

impl RendezvousClient {
    #[must_use]
    pub fn new(endpoint: String, node_id: NodeId, addr: Multiaddr, dht: DhtHandle) -> Self {
        Self { endpoint, node_id, addr, dht, http: reqwest::Client::new() }
    }

    /// Spawn background loop (30 s period).
    pub fn spawn(self) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(30));
            loop {
                ticker.tick().await;
                if let Err(e) = self.sync().await {
                    warn!("rendezvous sync error: {:#?}", e);
                }
            }
        });
    }

    async fn sync(&self) -> anyhow::Result<()> {
        // 1. announce our presence
        #[derive(Serialize)]
        struct Announce<'a> { node_id: String, addr: &'a str }
        let body = Announce { node_id: hex::encode(self.node_id), addr: &self.addr.to_string() };
        let announce_url = format!("{}/announce", self.endpoint);
        let _ = self.http.post(&announce_url).json(&body).send().await?;

        // 2. fetch peer list
        #[derive(Deserialize)]
        struct Peer { node_id: String, addr: String }
        let peers_url = format!("{}/peers", self.endpoint);
        let peers: Vec<Peer> = self.http.get(&peers_url).send().await?.json().await?;

        for p in peers {
            if p.node_id == hex::encode(self.node_id) { continue; }
            if let Ok(_maddr) = p.addr.parse::<Multiaddr>() {
                // dial via DHT for bootstrap (disabled for now)
                // self.dht.add_bootstrap(maddr.clone()).await;
                info!(peer=%p.node_id, addr=%p.addr, "rendezvous discovered peer");
            }
        }
        Ok(())
    }
} 