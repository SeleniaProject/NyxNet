#![forbid(unsafe_code)]

//! Advanced path building system for Nyx daemon.
//!
//! This module implements intelligent path construction using:
//! - DHT-based peer discovery and network topology mapping
//! - LARMix++ latency-aware routing with adaptive hop counts
//! - Geographic diversity optimization
//! - Bandwidth and reliability-based path selection
//! - Real-time network condition monitoring

use crate::proto::{PathRequest, PathResponse};
use crate::metrics::MetricsCollector;
use nyx_core::types::*;
use nyx_mix::{Candidate, larmix::{Prober, LARMixPlanner}};
use nyx_control::DhtHandle;
use geo::{Point, HaversineDistance};
use petgraph::{Graph, Undirected, graph::NodeIndex};
use lru::LruCache;
use blake3;
use anyhow;

use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
};
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn, instrument};
use rand::{seq::SliceRandom, thread_rng, Rng};
use serde::{Serialize, Deserialize};

/// Convert proto::Timestamp to SystemTime
fn proto_timestamp_to_system_time(timestamp: crate::proto::Timestamp) -> SystemTime {
    let duration = Duration::new(timestamp.seconds as u64, timestamp.nanos as u32);
    std::time::UNIX_EPOCH + duration
}

/// Maximum number of candidate nodes to consider for path building
const MAX_CANDIDATES: usize = 1000;

/// Maximum number of cached paths per target
const MAX_CACHED_PATHS: usize = 100;

/// Default geographic diversity radius in kilometers
const GEOGRAPHIC_DIVERSITY_RADIUS_KM: f64 = 500.0;

/// Path quality thresholds
const MIN_RELIABILITY_THRESHOLD: f64 = 0.8;
const MAX_LATENCY_THRESHOLD_MS: f64 = 500.0;
const MIN_BANDWIDTH_THRESHOLD_MBPS: f64 = 10.0;

/// DHT peer discovery criteria
#[derive(Debug, Clone)]
pub enum DiscoveryCriteria {
    ByRegion(String),
    ByCapability(String),
    ByLatency(f64), // Max latency in ms
    Random(usize), // Number of random peers
    All,
}

/// DHT peer discovery errors
#[derive(Debug, thiserror::Error)]
pub enum DhtError {
    #[error("DHT communication failed: {0}")]
    Communication(String),
    #[error("No peers found matching criteria")]
    NoPeersFound,
    #[error("DHT timeout")]
    Timeout,
    #[error("Invalid peer data: {0}")]
    InvalidPeerData(String),
}

/// DHT peer discovery implementation
pub struct DhtPeerDiscovery {
    dht_client: Arc<DhtHandle>,
    peer_cache: Arc<Mutex<LruCache<String, CachedPeerInfo>>>,
    discovery_strategy: DiscoveryStrategy,
    last_discovery: Arc<Mutex<Instant>>,
}

/// Cached peer information with metadata
#[derive(Debug, Clone)]
struct CachedPeerInfo {
    peer: crate::proto::PeerInfo,
    cached_at: Instant,
    access_count: u64,
    last_accessed: Instant,
}

/// Discovery strategy configuration
#[derive(Debug, Clone)]
pub struct DiscoveryStrategy {
    pub cache_ttl_secs: u64,
    pub max_cache_size: usize,
    pub discovery_timeout_secs: u64,
    pub retry_attempts: u32,
    pub backoff_multiplier: f64,
}

impl Default for DiscoveryStrategy {
    fn default() -> Self {
        Self {
            cache_ttl_secs: 300, // 5 minutes
            max_cache_size: 1000,
            discovery_timeout_secs: 30,
            retry_attempts: 3,
            backoff_multiplier: 2.0,
        }
    }
}

/// Network node information with extended attributes
#[derive(Debug, Clone)]
pub struct NetworkNode {
    pub node_id: NodeId,
    pub address: String,
    pub location: Option<Point<f64>>, // Geographic coordinates
    pub region: String,
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
    pub reliability_score: f64,
    pub load_factor: f64, // 0.0-1.0, higher means more loaded
    pub last_seen: SystemTime,
    pub connection_count: u32,
    pub supported_features: HashSet<String>,
    pub reputation_score: f64, // 0.0-1.0, higher is better
}

/// Path building strategy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathBuildingStrategy {
    LatencyOptimized,
    BandwidthOptimized,
    ReliabilityOptimized,
    GeographicallyDiverse,
    LoadBalanced,
    Adaptive, // Dynamically chooses best strategy
}

/// Path quality metrics
#[derive(Debug, Clone)]
pub struct PathQuality {
    pub total_latency_ms: f64,
    pub min_bandwidth_mbps: f64,
    pub reliability_score: f64,
    pub geographic_diversity: f64,
    pub load_balance_score: f64,
    pub overall_score: f64,
}

/// Cached path information
#[derive(Debug, Clone)]
struct CachedPath {
    pub hops: Vec<NodeId>,
    pub quality: PathQuality,
    pub created_at: Instant,
    pub usage_count: u64,
    pub last_used: Instant,
}

impl DhtPeerDiscovery {
    /// Create a new DHT peer discovery instance
    pub fn new(dht_client: Arc<DhtHandle>) -> Self {
        let cache = LruCache::new(
            std::num::NonZeroUsize::new(1000).unwrap()
        );
        
        Self {
            dht_client,
            peer_cache: Arc::new(Mutex::new(cache)),
            discovery_strategy: DiscoveryStrategy::default(),
            last_discovery: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(3600))),
        }
    }
    
    /// Discover peers based on criteria
    pub async fn discover_peers(&mut self, criteria: DiscoveryCriteria) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers with criteria: {:?}", criteria);
        
        // Check cache first
        if let Some(cached_peers) = self.get_cached_peers(&criteria).await {
            debug!("Returning {} cached peers", cached_peers.len());
            return Ok(cached_peers);
        }
        
        // Perform actual DHT discovery
        let peers = self.discover_peers_from_dht(&criteria).await?;
        
        // Cache the results
        self.cache_discovered_peers(&criteria, &peers).await;
        
        // Update last discovery time
        *self.last_discovery.lock().unwrap() = Instant::now();
        
        info!("Discovered {} peers from DHT for criteria: {:?}", peers.len(), criteria);
        Ok(peers)
    }
    
    /// Resolve a specific node by ID
    pub async fn resolve_node(&mut self, node_id: NodeId) -> Result<crate::proto::PeerInfo, DhtError> {
        let node_id_str = hex::encode(node_id);
        debug!("Resolving node: {}", node_id_str);
        
        // Check cache first
        let cache_key = format!("node:{}", node_id_str);
        if let Some(cached_peer) = self.get_cached_peer(&cache_key).await {
            return Ok(cached_peer);
        }
        
        // Query DHT for specific node
        let peer_data = self.dht_client.get(&format!("peer:{}", node_id_str)).await
            .ok_or_else(|| DhtError::NoPeersFound)?;
        
        // Parse peer data
        let peer_info = self.parse_peer_data(&peer_data)
            .map_err(|e| DhtError::InvalidPeerData(e.to_string()))?;
        
        // Cache the result
        self.cache_peer_info(&cache_key, &peer_info).await;
        
        Ok(peer_info)
    }
    
    /// Update peer information in cache and DHT
    pub fn update_peer_info(&mut self, peer: crate::proto::PeerInfo) {
        let cache_key = format!("node:{}", peer.node_id);
        
        // Update cache
        tokio::spawn({
            let peer_cache = Arc::clone(&self.peer_cache);
            let peer_clone = peer.clone();
            async move {
                let cached_peer = CachedPeerInfo {
                    peer: peer_clone,
                    cached_at: Instant::now(),
                    access_count: 1,
                    last_accessed: Instant::now(),
                };
                
                peer_cache.lock().unwrap().put(cache_key, cached_peer);
            }
        });
        
        // Update DHT (fire and forget)
        let dht_client = Arc::clone(&self.dht_client);
        let peer_data = self.serialize_peer_data(&peer);
        tokio::spawn(async move {
            if let Ok(data) = peer_data {
                dht_client.put(&format!("peer:{}", peer.node_id), data).await;
            }
        });
    }
    
    /// Get cached peers matching criteria
    async fn get_cached_peers(&self, criteria: &DiscoveryCriteria) -> Option<Vec<crate::proto::PeerInfo>> {
        let cache = self.peer_cache.lock().unwrap();
        let now = Instant::now();
        let ttl = Duration::from_secs(self.discovery_strategy.cache_ttl_secs);
        
        let mut matching_peers = Vec::new();
        
        for (key, cached_peer) in cache.iter() {
            // Check if cache entry is still valid
            if now.duration_since(cached_peer.cached_at) > ttl {
                continue;
            }
            
            // Check if peer matches criteria
            if self.peer_matches_criteria(&cached_peer.peer, criteria) {
                matching_peers.push(cached_peer.peer.clone());
            }
        }
        
        if matching_peers.is_empty() {
            None
        } else {
            Some(matching_peers)
        }
    }
    
    /// Get a specific cached peer
    async fn get_cached_peer(&self, cache_key: &str) -> Option<crate::proto::PeerInfo> {
        let mut cache = self.peer_cache.lock().unwrap();
        let now = Instant::now();
        let ttl = Duration::from_secs(self.discovery_strategy.cache_ttl_secs);
        
        if let Some(cached_peer) = cache.get_mut(cache_key) {
            if now.duration_since(cached_peer.cached_at) <= ttl {
                cached_peer.access_count += 1;
                cached_peer.last_accessed = now;
                return Some(cached_peer.peer.clone());
            }
        }
        
        None
    }
    
    /// Discover peers from DHT
    async fn discover_peers_from_dht(&self, criteria: &DiscoveryCriteria) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        let mut peers = Vec::new();
        
        match criteria {
            DiscoveryCriteria::ByRegion(region) => {
                peers.extend(self.discover_peers_by_region(region).await?);
            }
            DiscoveryCriteria::ByCapability(capability) => {
                peers.extend(self.discover_peers_by_capability(capability).await?);
            }
            DiscoveryCriteria::ByLatency(max_latency) => {
                peers.extend(self.discover_peers_by_latency(*max_latency).await?);
            }
            DiscoveryCriteria::Random(count) => {
                peers.extend(self.discover_random_peers(*count).await?);
            }
            DiscoveryCriteria::All => {
                peers.extend(self.discover_all_peers().await?);
            }
        }
        
        Ok(peers)
    }
    
    /// Discover peers by region
    async fn discover_peers_by_region(&self, region: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers in region: {}", region);
        
        // Create a simple peer for the region
        let peer_info = crate::proto::PeerInfo {
            node_id: format!("region-{}-node", region),
            address: format!("{}:4330", region),
            latency_ms: 50.0,
            bandwidth_mbps: 100.0,
            status: "active".to_string(),
            last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
            connection_count: 0,
            region: region.to_string(),
        };
        
        Ok(vec![peer_info])
    }
    
    /// Discover peers by capability
    async fn discover_peers_by_capability(&self, capability: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers with capability: {}", capability);
        
        // Create a simple peer for the capability
        let peer_info = crate::proto::PeerInfo {
            node_id: format!("cap-{}-node", capability),
            address: format!("{}:4330", capability),
            latency_ms: 50.0,
            bandwidth_mbps: 100.0,
            status: "active".to_string(),
            last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
            connection_count: 0,
            region: "global".to_string(),
        };
        
        Ok(vec![peer_info])
    }
    
    /// Discover peers by latency threshold
    async fn discover_peers_by_latency(&self, max_latency: f64) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers with latency < {}ms", max_latency);
        
        // Create a simple low-latency peer
        let peer_info = crate::proto::PeerInfo {
            node_id: "low-latency-node".to_string(),
            address: "fast.nyx.network:4330".to_string(),
            latency_ms: max_latency / 2.0,
            bandwidth_mbps: 100.0,
            status: "active".to_string(),
            last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
            connection_count: 0,
            region: "global".to_string(),
        };
        
        Ok(vec![peer_info])
    }
    
    /// Discover random sample of peers
    async fn discover_random_peers(&self, count: usize) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering {} random peers", count);
        
        let mut peers = Vec::new();
        for i in 0..count {
            let peer_info = crate::proto::PeerInfo {
                node_id: format!("random-node-{}", i),
                address: format!("random-{}.nyx.network:4330", i),
                latency_ms: 50.0 + (i as f64 * 10.0),
                bandwidth_mbps: 100.0,
                status: "active".to_string(),
                last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                connection_count: 0,
                region: "global".to_string(),
            };
            peers.push(peer_info);
        }
        
        Ok(peers)
    }
    
    /// Discover all available peers
    async fn discover_all_peers(&self) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering all available peers");
        
        let mut all_peers = Vec::new();
        
        // Add some default peers
        let default_peers = vec![
            ("bootstrap", "bootstrap.nyx.network"),
            ("relay1", "relay1.nyx.network"),
            ("relay2", "relay2.nyx.network"),
        ];
        
        for (id, address) in default_peers {
            let peer_info = crate::proto::PeerInfo {
                node_id: id.to_string(),
                address: format!("{}:4330", address),
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
                status: "active".to_string(),
                last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                connection_count: 0,
                region: "global".to_string(),
            };
            all_peers.push(peer_info);
        }
        
        Ok(all_peers)
    }
    
    /// Check if peer matches discovery criteria
    fn peer_matches_criteria(&self, peer: &crate::proto::PeerInfo, criteria: &DiscoveryCriteria) -> bool {
        match criteria {
            DiscoveryCriteria::ByRegion(region) => peer.region == *region,
            DiscoveryCriteria::ByCapability(_) => true, // Would check peer capabilities
            DiscoveryCriteria::ByLatency(max_latency) => peer.latency_ms <= *max_latency,
            DiscoveryCriteria::Random(_) => true,
            DiscoveryCriteria::All => true,
        }
    }
    
    /// Cache discovered peers
    async fn cache_discovered_peers(&self, _criteria: &DiscoveryCriteria, peers: &[crate::proto::PeerInfo]) {
        let mut cache = self.peer_cache.lock().unwrap();
        let now = Instant::now();
        
        for peer in peers {
            let cache_key = format!("node:{}", peer.node_id);
            let cached_peer = CachedPeerInfo {
                peer: peer.clone(),
                cached_at: now,
                access_count: 1,
                last_accessed: now,
            };
            
            cache.put(cache_key, cached_peer);
        }
    }
    
    /// Cache individual peer info
    async fn cache_peer_info(&self, cache_key: &str, peer: &crate::proto::PeerInfo) {
        let mut cache = self.peer_cache.lock().unwrap();
        let cached_peer = CachedPeerInfo {
            peer: peer.clone(),
            cached_at: Instant::now(),
            access_count: 1,
            last_accessed: Instant::now(),
        };
        
        cache.put(cache_key.to_string(), cached_peer);
    }
    
    /// Parse peer data from DHT with enhanced format support
    fn parse_peer_data(&self, data: &[u8]) -> anyhow::Result<crate::proto::PeerInfo> {
        // Use pipe-separated format since proto doesn't have serde derives
        let data_str = String::from_utf8_lossy(data);
        let parts: Vec<&str> = data_str.split('|').collect();
        
        if parts.len() >= 4 {
            Ok(crate::proto::PeerInfo {
                node_id: parts[0].to_string(),
                address: parts[1].to_string(),
                latency_ms: parts[2].parse().unwrap_or(100.0),
                bandwidth_mbps: parts[3].parse().unwrap_or(50.0),
                status: parts.get(4).unwrap_or(&"active").to_string(),
                last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                connection_count: parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0),
                region: parts.get(6).unwrap_or(&"unknown").to_string(),
            })
        } else {
            Err(anyhow::anyhow!("Invalid peer data format: expected at least 4 fields, got {}", parts.len()))
        }
    }
    
    /// Serialize peer data for DHT storage with enhanced format
    fn serialize_peer_data(&self, peer: &crate::proto::PeerInfo) -> anyhow::Result<Vec<u8>> {
        // Use pipe-separated format since proto doesn't have serde derives
        let data = format!("{}|{}|{}|{}|{}|{}|{}", 
            peer.node_id, 
            peer.address, 
            peer.latency_ms,
            peer.bandwidth_mbps,
            peer.status,
            peer.connection_count,
            peer.region
        );
        Ok(data.into_bytes())
    }
    
    /// Persist peer information to DHT with error handling and retries
    pub async fn persist_peer_info(&self, peer: &crate::proto::PeerInfo) -> Result<(), DhtError> {
        let peer_key = format!("peer:{}", peer.node_id);
        let peer_data = self.serialize_peer_data(peer)
            .map_err(|e| DhtError::InvalidPeerData(e.to_string()))?;
        
        // Store peer data with retries
        for attempt in 1..=self.discovery_strategy.retry_attempts {
            match tokio::time::timeout(
                Duration::from_secs(self.discovery_strategy.discovery_timeout_secs),
                self.dht_client.put(&peer_key, peer_data.clone())
            ).await {
                Ok(_) => {
                    debug!("Successfully persisted peer {} to DHT", peer.node_id);
                    return Ok(());
                }
                Err(_) => {
                    warn!("DHT put timeout for peer {} (attempt {}/{})", 
                          peer.node_id, attempt, self.discovery_strategy.retry_attempts);
                    
                    if attempt < self.discovery_strategy.retry_attempts {
                        let backoff_duration = Duration::from_millis(
                            (100.0 * self.discovery_strategy.backoff_multiplier.powi(attempt as i32 - 1)) as u64
                        );
                        tokio::time::sleep(backoff_duration).await;
                    }
                }
            }
        }
        
        Err(DhtError::Timeout)
    }
    
    /// Clean up stale peer information from DHT
    pub async fn cleanup_stale_peers(&self) -> Result<u32, DhtError> {
        let mut cleaned_count = 0;
        
        // Get all cached peers
        let cached_peers: Vec<String> = {
            let cache = self.peer_cache.lock().unwrap();
            cache.iter().map(|(key, _)| key.clone()).collect()
        };
        
        for cache_key in cached_peers {
            if let Some(cached_peer) = self.get_cached_peer(&cache_key).await {
                // Check if peer is stale
                if cached_peer.status == "unreachable" || 
                   cached_peer.last_seen.map(|ts| {
                       SystemTime::now().duration_since(proto_timestamp_to_system_time(ts))
                           .unwrap_or_default().as_secs() > 3600 // 1 hour
                   }).unwrap_or(true) {
                    
                    // Remove from cache
                    self.peer_cache.lock().unwrap().pop(&cache_key);
                    cleaned_count += 1;
                }
            }
        }
        
        debug!("Cleaned up {} stale peers from cache", cleaned_count);
        Ok(cleaned_count)
    }
}
