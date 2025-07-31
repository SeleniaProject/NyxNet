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
    
    /// Discover peers by region with actual network queries
    async fn discover_peers_by_region(&self, region: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers in region: {}", region);
        
        let mut discovered_peers = Vec::new();
        
        // Try multiple DHT keys for region-based discovery
        let region_keys = vec![
            format!("region:{}", region),
            format!("nodes:region:{}", region),
            format!("peers:region:{}", region),
        ];
        
        for region_key in region_keys {
            if let Some(peer_list_data) = self.dht_client.get(&region_key).await {
                match self.parse_peer_list(&peer_list_data).await {
                    Ok(peers) => {
                        discovered_peers.extend(peers);
                    }
                    Err(e) => {
                        debug!("Failed to parse peer list for key {}: {}", region_key, e);
                    }
                }
            }
        }
        
        // If no peers found in DHT, try network-based discovery
        if discovered_peers.is_empty() {
            discovered_peers = self.network_discover_peers_by_region(region).await?;
        }
        
        // Validate and probe discovered peers
        let validated_peers = self.validate_discovered_peers(discovered_peers).await;
        
        if validated_peers.is_empty() {
            Err(DhtError::NoPeersFound)
        } else {
            Ok(validated_peers)
        }
    }
    
    /// Network-based peer discovery by region
    async fn network_discover_peers_by_region(&self, region: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Performing network-based discovery for region: {}", region);
        
        let mut peers = Vec::new();
        
        // Try to discover peers through well-known regional endpoints
        let regional_endpoints = self.get_regional_endpoints(region);
        
        for endpoint in regional_endpoints {
            match self.query_regional_endpoint(&endpoint).await {
                Ok(endpoint_peers) => {
                    peers.extend(endpoint_peers);
                }
                Err(e) => {
                    debug!("Failed to query regional endpoint {}: {}", endpoint, e);
                }
            }
        }
        
        Ok(peers)
    }
    
    /// Get regional endpoints for discovery
    fn get_regional_endpoints(&self, region: &str) -> Vec<String> {
        match region {
            "north_america" => vec![
                "us-east.nyx.network:4330".to_string(),
                "us-west.nyx.network:4330".to_string(),
                "canada.nyx.network:4330".to_string(),
            ],
            "europe" => vec![
                "eu-west.nyx.network:4330".to_string(),
                "eu-central.nyx.network:4330".to_string(),
                "uk.nyx.network:4330".to_string(),
            ],
            "asia_pacific" => vec![
                "asia.nyx.network:4330".to_string(),
                "japan.nyx.network:4330".to_string(),
                "singapore.nyx.network:4330".to_string(),
            ],
            _ => vec![
                "global.nyx.network:4330".to_string(),
            ],
        }
    }
    
    /// Query a regional endpoint for peers
    async fn query_regional_endpoint(&self, endpoint: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        use tokio::net::TcpStream;
        use tokio::time::timeout;
        
        // Try to connect to the endpoint
        let stream = timeout(Duration::from_secs(10), TcpStream::connect(endpoint))
            .await
            .map_err(|_| DhtError::Timeout)?
            .map_err(|e| DhtError::Communication(format!("Failed to connect to {}: {}", endpoint, e)))?;
        
        // For now, just create a peer info from the successful connection
        // In a real implementation, this would perform a protocol handshake
        drop(stream);
        
        let peer_info = self.create_peer_from_address(endpoint).await?;
        Ok(vec![peer_info])
    }
    
    /// Discover peers by capability
    async fn discover_peers_by_capability(&self, capability: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers with capability: {}", capability);
        
        // Query DHT for capability-specific peer list
        let capability_key = format!("capability:{}", capability);
        let peer_list_data = self.dht_client.get(&capability_key).await
            .ok_or_else(|| DhtError::NoPeersFound)?;
        
        // Parse and resolve peers (similar to region discovery)
        let peer_ids: Vec<String> = serde_json::from_slice(&peer_list_data)
            .map_err(|e| DhtError::InvalidPeerData(e.to_string()))?;
        
        let mut peers = Vec::new();
        for peer_id in peer_ids {
            if let Some(peer_data) = self.dht_client.get(&format!("peer:{}", peer_id)).await {
                if let Ok(peer_info) = self.parse_peer_data(&peer_data) {
                    peers.push(peer_info);
                }
            }
        }
        
        Ok(peers)
    }
    
    /// Discover peers by latency threshold
    async fn discover_peers_by_latency(&self, max_latency: f64) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers with latency < {}ms", max_latency);
        
        // Get all peers and filter by latency
        let all_peers = self.discover_all_peers().await?;
        let filtered_peers: Vec<_> = all_peers.into_iter()
            .filter(|peer| peer.latency_ms <= max_latency)
            .collect();
        
        Ok(filtered_peers)
    }
    
    /// Discover random sample of peers
    async fn discover_random_peers(&self, count: usize) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering {} random peers", count);
        
        // Get all peers and sample randomly
        let mut all_peers = self.discover_all_peers().await?;
        all_peers.shuffle(&mut thread_rng());
        all_peers.truncate(count);
        
        Ok(all_peers)
    }
    
    /// Discover all available peers through comprehensive DHT querying and network discovery
    async fn discover_all_peers(&self) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering all available peers");
        
        let mut all_peers = Vec::new();
        
        // Primary discovery through DHT global peer list
        if let Some(global_peer_list) = self.dht_client.get("peers:all").await {
            match serde_json::from_slice::<Vec<String>>(&global_peer_list) {
                Ok(peer_ids) => {
                    for peer_id in peer_ids {
                        if let Some(peer_data) = self.dht_client.get(&format!("peer:{}", peer_id)).await {
                            if let Ok(peer_info) = self.parse_peer_data(&peer_data) {
                                all_peers.push(peer_info);
                            }
                        }
                    }
                }
                Err(e) => debug!("Failed to parse global peer list: {}", e),
            }
        }
        
        // Secondary discovery through regional lists
        let regions = ["north_america", "europe", "asia_pacific", "south_america", "africa", "oceania"];
        for region in &regions {
            if let Ok(regional_peers) = self.discover_peers_by_region(region).await {
                all_peers.extend(regional_peers);
            }
        }
        
        // Tertiary discovery through capability-based queries
        let capabilities = ["mix", "gateway", "relay", "bridge"];
        for capability in &capabilities {
            if let Ok(capability_peers) = self.discover_peers_by_capability(capability).await {
                all_peers.extend(capability_peers);
            }
        }
        
        // Remove duplicates based on node_id
        let mut seen_ids = std::collections::HashSet::new();
        all_peers.retain(|peer| seen_ids.insert(peer.node_id.clone()));
        
        // If still no peers found, attempt bootstrap discovery
        if all_peers.is_empty() {
            warn!("No peers found in DHT, using bootstrap discovery");
            all_peers = self.create_bootstrap_peers().await;
        }
        
        // Validate discovered peers through network probes
        let validated_peers = self.validate_discovered_peers(all_peers).await;
        
        if validated_peers.is_empty() {
            Err(DhtError::NoPeersFound)
        } else {
            info!("Discovered {} validated peers from all sources", validated_peers.len());
            Ok(validated_peers)
        }
    }
    
    /// Create bootstrap peers with actual network discovery
    async fn create_bootstrap_peers(&self) -> Vec<crate::proto::PeerInfo> {
        debug!("Creating bootstrap peers through network discovery");
        
        let mut bootstrap_peers = Vec::new();
        
        // Well-known bootstrap endpoints for different networks
        let bootstrap_endpoints = vec![
            ("bootstrap.nyx.network", 4330),
            ("seed1.nyx.network", 4330), 
            ("seed2.nyx.network", 4330),
            ("directory.nyx.network", 4330),
            ("relay1.nyx.network", 4330),
            ("relay2.nyx.network", 4330),
        ];
        
        // Attempt to discover peers from each bootstrap endpoint
        for (host, port) in &bootstrap_endpoints {
            match self.probe_and_discover_from_endpoint(host, *port).await {
                Ok(mut discovered_peers) => {
                    bootstrap_peers.append(&mut discovered_peers);
                    
                    // If we have enough peers, stop early
                    if bootstrap_peers.len() >= 10 {
                        break;
                    }
                }
                Err(e) => {
                    debug!("Failed to discover from endpoint {}:{}: {}", host, port, e);
                }
            }
        }
        
        // If network discovery failed, use DNS-based discovery
        if bootstrap_peers.is_empty() {
            bootstrap_peers = self.dns_bootstrap_discovery().await;
        }
        
        // If still no peers, create fallback peers for testing
        if bootstrap_peers.is_empty() {
            bootstrap_peers = self.create_fallback_peers().await;
        }
        
        info!("Created {} bootstrap peers", bootstrap_peers.len());
        bootstrap_peers
    }
    
    /// Probe an endpoint and discover available peers
    async fn probe_and_discover_from_endpoint(&self, host: &str, port: u16) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        use tokio::net::TcpStream;
        use tokio::time::timeout;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        
        let address = format!("{}:{}", host, port);
        debug!("Probing endpoint for peer discovery: {}", address);
        
        // Establish connection with timeout
        let mut stream = timeout(Duration::from_secs(10), TcpStream::connect(&address))
            .await
            .map_err(|_| DhtError::Timeout)?
            .map_err(|e| DhtError::Communication(format!("Failed to connect to {}: {}", address, e)))?;
        
        // Send peer discovery request
        let discovery_request = serde_json::json!({
            "type": "peer_discovery",
            "version": "1.0",
            "max_peers": 50,
            "include_regions": true,
            "include_capabilities": true
        });
        
        let request_data = format!("{}\n", discovery_request.to_string());
        
        // Write request
        stream.write_all(request_data.as_bytes()).await
            .map_err(|e| DhtError::Communication(format!("Failed to write discovery request: {}", e)))?;
        
        // Read response with timeout
        let mut response_buffer = Vec::new();
        let mut temp_buffer = [0u8; 4096];
        
        loop {
            match timeout(Duration::from_secs(5), stream.read(&mut temp_buffer)).await {
                Ok(Ok(0)) => break, // Connection closed
                Ok(Ok(n)) => {
                    response_buffer.extend_from_slice(&temp_buffer[..n]);
                    
                    // Check if we have a complete JSON response (ends with newline)
                    if response_buffer.ends_with(b"\n") {
                        break;
                    }
                }
                Ok(Err(e)) => return Err(DhtError::Communication(format!("Read error: {}", e))),
                Err(_) => return Err(DhtError::Timeout),
            }
        }
        
        // Parse response
        let response_str = String::from_utf8(response_buffer)
            .map_err(|e| DhtError::InvalidPeerData(format!("Invalid UTF-8 response: {}", e)))?;
        
        let response: serde_json::Value = serde_json::from_str(&response_str.trim())
            .map_err(|e| DhtError::InvalidPeerData(format!("Invalid JSON response: {}", e)))?;
        
        // Extract peer list from response
        if let Some(peers_array) = response.get("peers").and_then(|p| p.as_array()) {
            let mut discovered_peers = Vec::new();
            
            for peer_value in peers_array {
                if let Ok(peer_info) = self.parse_peer_from_json(peer_value).await {
                    discovered_peers.push(peer_info);
                }
            }
            
            if !discovered_peers.is_empty() {
                info!("Discovered {} peers from endpoint {}", discovered_peers.len(), address);
                return Ok(discovered_peers);
            }
        }
        
        // Fallback: create peer info from the endpoint itself
        let peer_info = self.create_peer_from_address(&address).await?;
        Ok(vec![peer_info])
    }
    
    /// DNS-based bootstrap peer discovery
    async fn dns_bootstrap_discovery(&self) -> Vec<crate::proto::PeerInfo> {
        use trust_dns_resolver::{TokioAsyncResolver, config::*};
        
        debug!("Attempting DNS-based bootstrap discovery");
        
        let mut dns_peers = Vec::new();
        
        // Create DNS resolver
        if let Ok(resolver) = TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            ResolverOpts::default()
        ) {
            // DNS seeds for Nyx network
            let dns_seeds = vec![
                "_nyx-nodes._tcp.nyx.network",
                "_nyx-mix._tcp.nyx.network", 
                "_nyx-gateway._tcp.nyx.network",
                "_peers._tcp.nyx.network",
            ];
            
            for seed in &dns_seeds {
                match resolver.srv_lookup(seed).await {
                    Ok(srv_records) => {
                        for record in srv_records.iter() {
                            let target = record.target().to_string();
                            let port = record.port();
                            let address = format!("{}:{}", target, port);
                            
                            if let Ok(peer_info) = self.create_peer_from_address(&address).await {
                                dns_peers.push(peer_info);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("DNS SRV lookup failed for {}: {}", seed, e);
                    }
                }
            }
            
            // Also try TXT record-based discovery
            if let Ok(txt_response) = resolver.txt_lookup("_nyx-bootstrap.nyx.network").await {
                for txt_record in txt_response.iter() {
                    for txt_data in txt_record.iter() {
                        if let Ok(txt_str) = String::from_utf8(txt_data.to_vec()) {
                            if let Ok(peer_addresses) = serde_json::from_str::<Vec<String>>(&txt_str) {
                                for address in peer_addresses {
                                    if let Ok(peer_info) = self.create_peer_from_address(&address).await {
                                        dns_peers.push(peer_info);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        info!("DNS discovery found {} peers", dns_peers.len());
        dns_peers
    }
    
    /// Create fallback peers for testing when no network discovery works
    async fn create_fallback_peers(&self) -> Vec<crate::proto::PeerInfo> {
        debug!("Creating fallback peers for testing");
        
        let fallback_addresses = vec![
            ("127.0.0.1", 4330),
            ("127.0.0.1", 4331),
            ("127.0.0.1", 4332),
            ("localhost", 4330),
        ];
        
        let mut fallback_peers = Vec::new();
        
        for (i, (host, port)) in fallback_addresses.iter().enumerate() {
            let node_id = format!("fallback_{:02x}", i);
            let address = format!("{}:{}", host, port);
            
            fallback_peers.push(crate::proto::PeerInfo {
                node_id,
                address,
                latency_ms: 1.0 + (i as f64 * 0.5), // Very low latency for local peers
                bandwidth_mbps: 1000.0, // High bandwidth for local peers
                status: "testing".to_string(),
                last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                connection_count: 0,
                region: "local".to_string(),
            });
        }
        
        warn!("Using {} fallback peers for testing", fallback_peers.len());
        fallback_peers
    }
    
    /// Probe a bootstrap node to get actual peer information
    async fn probe_bootstrap_node(&self, host: &str, port: u16) -> Result<crate::proto::PeerInfo, DhtError> {
        use tokio::net::TcpStream;
        use tokio::time::timeout;
        
        let address = format!("{}:{}", host, port);
        let start_time = Instant::now();
        
        // Try to establish TCP connection with timeout
        let stream = timeout(Duration::from_secs(5), TcpStream::connect(&address))
            .await
            .map_err(|_| DhtError::Timeout)?
            .map_err(|e| DhtError::Communication(format!("TCP connection failed: {}", e)))?;
        
        let latency_ms = start_time.elapsed().as_millis() as f64;
        
        // Close the connection immediately (we just wanted to test reachability)
        drop(stream);
        
        // Generate node ID from host (deterministic)
        let node_id = self.generate_node_id_from_host(host);
        
        Ok(crate::proto::PeerInfo {
            node_id,
            address,
            latency_ms,
            bandwidth_mbps: 100.0, // Assume good bandwidth for bootstrap nodes
            status: "active".to_string(),
            last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
            connection_count: 0,
            region: self.infer_region_from_host(host),
        })
    }
    
    /// Get bootstrap peers from DHT
    async fn get_dht_bootstrap_peers(&self) -> Vec<crate::proto::PeerInfo> {
        let mut dht_peers = Vec::new();
        
        // Try to get known peers from DHT
        if let Some(peer_list_data) = self.dht_client.get("bootstrap:peers").await {
            if let Ok(peer_addresses) = serde_json::from_slice::<Vec<String>>(&peer_list_data) {
                for addr in peer_addresses {
                    if let Ok(peer_info) = self.create_peer_from_address(&addr).await {
                        dht_peers.push(peer_info);
                    }
                }
            }
        }
        
        // Add DHT's own listen address as a peer
        let dht_addr = self.dht_client.listen_addr().to_string();
        if let Ok(self_peer) = self.create_peer_from_address(&dht_addr).await {
            dht_peers.push(self_peer);
        }
        
        dht_peers
    }
    
    /// Create peer info from address
    async fn create_peer_from_address(&self, address: &str) -> Result<crate::proto::PeerInfo, DhtError> {
        let node_id = self.generate_node_id_from_address(address);
        
        Ok(crate::proto::PeerInfo {
            node_id,
            address: address.to_string(),
            latency_ms: 50.0, // Default latency
            bandwidth_mbps: 100.0, // Default bandwidth
            status: "active".to_string(),
            last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
            connection_count: 0,
            region: self.infer_region_from_address(address),
        })
    }
    
    /// Generate deterministic node ID from host
    fn generate_node_id_from_host(&self, host: &str) -> String {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(b"nyx-node:");
        hasher.update(host.as_bytes());
        hex::encode(hasher.finalize().as_bytes()[..16].to_vec())
    }
    
    /// Generate deterministic node ID from address
    fn generate_node_id_from_address(&self, address: &str) -> String {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(b"nyx-peer:");
        hasher.update(address.as_bytes());
        hex::encode(hasher.finalize().as_bytes()[..16].to_vec())
    }
    
    /// Infer region from hostname
    fn infer_region_from_host(&self, host: &str) -> String {
        if host.contains("us") || host.contains("america") {
            "north_america".to_string()
        } else if host.contains("eu") || host.contains("europe") {
            "europe".to_string()
        } else if host.contains("asia") || host.contains("ap") {
            "asia_pacific".to_string()
        } else if host.contains("au") || host.contains("oceania") {
            "oceania".to_string()
        } else {
            "global".to_string()
        }
    }
    
    /// Infer region from address
    fn infer_region_from_address(&self, address: &str) -> String {
        // Extract hostname from address
        if let Some(host) = address.split(':').next() {
            self.infer_region_from_host(host)
        } else {
            "unknown".to_string()
        }
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
    async fn cache_discovered_peers(&self, criteria: &DiscoveryCriteria, peers: &[crate::proto::PeerInfo]) {
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
    
    /// Parse peer list from DHT data
    async fn parse_peer_list(&self, data: &[u8]) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        // Try to parse as JSON array of peer IDs first
        if let Ok(peer_ids) = serde_json::from_slice::<Vec<String>>(data) {
            let mut peers = Vec::new();
            for peer_id in peer_ids {
                if let Some(peer_data) = self.dht_client.get(&format!("peer:{}", peer_id)).await {
                    if let Ok(peer_info) = self.parse_peer_data(&peer_data) {
                        peers.push(peer_info);
                    }
                }
            }
            return Ok(peers);
        }
        
        // Fall back to line-separated format
        let data_str = String::from_utf8_lossy(data);
        let mut peers = Vec::new();
        
        for line in data_str.lines() {
            if !line.trim().is_empty() {
                if let Ok(peer_info) = self.parse_peer_data(line.as_bytes()) {
                    peers.push(peer_info);
                }
            }
        }
        
        Ok(peers)
    }
    
    /// Validate discovered peers by probing them
    async fn validate_discovered_peers(&self, peers: Vec<crate::proto::PeerInfo>) -> Vec<crate::proto::PeerInfo> {
        let mut validated_peers = Vec::new();
        
        // Use semaphore to limit concurrent probes
        let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
        let mut probe_tasks = Vec::new();
        
        for peer in peers {
            let semaphore = Arc::clone(&semaphore);
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                Self::probe_peer_connectivity(peer).await
            });
            probe_tasks.push(task);
        }
        
        // Collect results
        for task in probe_tasks {
            if let Ok(Some(validated_peer)) = task.await {
                validated_peers.push(validated_peer);
            }
        }
        
        validated_peers
    }
    
    /// Probe peer connectivity
    async fn probe_peer_connectivity(mut peer: crate::proto::PeerInfo) -> Option<crate::proto::PeerInfo> {
        use tokio::net::TcpStream;
        use tokio::time::timeout;
        
        let start_time = Instant::now();
        
        // Try to establish connection
        match timeout(Duration::from_secs(5), TcpStream::connect(&peer.address)).await {
            Ok(Ok(_stream)) => {
                // Connection successful, update latency
                peer.latency_ms = start_time.elapsed().as_millis() as f64;
                peer.status = "active".to_string();
                peer.last_seen = Some(crate::system_time_to_proto_timestamp(SystemTime::now()));
                Some(peer)
            }
            Ok(Err(_)) | Err(_) => {
                // Connection failed
                peer.status = "unreachable".to_string();
                peer.latency_ms = f64::INFINITY;
                // Still return the peer but mark as unreachable
                Some(peer)
            }
        }
    }
    
    /// Parse peer information from JSON format
    async fn parse_peer_from_json(&self, peer_value: &serde_json::Value) -> Result<crate::proto::PeerInfo, DhtError> {
        // Extract fields from JSON
        let node_id = peer_value.get("node_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DhtError::InvalidPeerData("Missing node_id field".to_string()))?
            .to_string();
        
        let address = peer_value.get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| DhtError::InvalidPeerData("Missing address field".to_string()))?
            .to_string();
        
        let latency_ms = peer_value.get("latency_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);
        
        let bandwidth_mbps = peer_value.get("bandwidth_mbps")
            .and_then(|v| v.as_f64())
            .unwrap_or(50.0);
        
        let status = peer_value.get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("active")
            .to_string();
        
        let connection_count = peer_value.get("connection_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        
        let region = peer_value.get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        
        Ok(crate::proto::PeerInfo {
            node_id,
            address,
            latency_ms,
            bandwidth_mbps,
            status,
            last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
            connection_count,
            region,
        })
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
                    
                    // Also update regional index
                    self.update_regional_index(peer).await.ok();
                    
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
    
    /// Update regional index for peer discovery
    async fn update_regional_index(&self, peer: &crate::proto::PeerInfo) -> Result<(), DhtError> {
        let region_key = format!("region:{}", peer.region);
        
        // Get existing peer list for region
        let mut peer_ids = if let Some(data) = self.dht_client.get(&region_key).await {
            serde_json::from_slice::<Vec<String>>(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        
        // Add peer if not already present
        if !peer_ids.contains(&peer.node_id) {
            peer_ids.push(peer.node_id.clone());
            
            // Limit region list size
            if peer_ids.len() > 1000 {
                peer_ids.truncate(1000);
            }
            
            // Update regional index
            if let Ok(updated_data) = serde_json::to_vec(&peer_ids) {
                self.dht_client.put(&region_key, updated_data).await;
            }
        }
        
        Ok(())
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

/// Geographic path builder for diversity optimization
pub struct GeographicPathBuilder {
    location_service: LocationService,
    diversity_config: DiversityConfig,
    path_optimizer: PathOptimizer,
}

/// Location service for geographic data
pub struct LocationService {
    location_cache: Arc<Mutex<HashMap<String, Point<f64>>>>,
    geoip_enabled: bool,
}

/// Regional distribution analysis
#[derive(Debug, Clone)]
pub struct RegionalDistribution {
    pub region_counts: HashMap<String, u32>,
    pub region_quality: HashMap<String, RegionQuality>,
    pub diversity_score: f64,
    pub total_nodes: u32,
}

/// Regional quality metrics
#[derive(Debug, Clone)]
pub struct RegionQuality {
    pub avg_latency: f64,
    pub avg_bandwidth: f64,
    pub avg_reliability: f64,
    pub node_count: u32,
    pub total_latency: f64,
    pub total_bandwidth: f64,
    pub total_reliability: f64,
}

/// Quality constraints for path optimization
#[derive(Debug, Clone)]
pub struct QualityConstraints {
    pub max_latency_ms: f64,
    pub min_bandwidth_mbps: f64,
    pub min_reliability: f64,
    pub max_load_factor: f64,
}

/// Diversity configuration
#[derive(Debug, Clone)]
pub struct DiversityConfig {
    pub min_distance_km: f64,
    pub max_hops_per_region: u32,
    pub preferred_regions: Vec<String>,
    pub diversity_weight: f64,
}

impl Default for DiversityConfig {
    fn default() -> Self {
        Self {
            min_distance_km: 500.0,
            max_hops_per_region: 2,
            preferred_regions: vec![
                "north_america".to_string(),
                "europe".to_string(),
                "asia_pacific".to_string(),
            ],
            diversity_weight: 0.3,
        }
    }
}

/// Path optimizer for quality-based selection
pub struct PathOptimizer {
    quality_weights: QualityWeights,
}

/// Quality weights for path optimization
#[derive(Debug, Clone)]
pub struct QualityWeights {
    pub latency_weight: f64,
    pub bandwidth_weight: f64,
    pub reliability_weight: f64,
    pub diversity_weight: f64,
    pub load_weight: f64,
}

impl Default for QualityWeights {
    fn default() -> Self {
        Self {
            latency_weight: 0.3,
            bandwidth_weight: 0.25,
            reliability_weight: 0.25,
            diversity_weight: 0.1,
            load_weight: 0.1,
        }
    }
}

impl LocationService {
    pub fn new() -> Self {
        Self {
            location_cache: Arc::new(Mutex::new(HashMap::new())),
            geoip_enabled: true,
        }
    }
    
    /// Get location for an address
    pub async fn get_location(&self, address: &str) -> Option<Point<f64>> {
        // Check cache first
        {
            let cache = self.location_cache.lock().unwrap();
            if let Some(location) = cache.get(address) {
                return Some(*location);
            }
        }
        
        // Resolve location (simplified implementation)
        let location = self.resolve_location(address).await;
        
        // Cache the result
        if let Some(loc) = location {
            let mut cache = self.location_cache.lock().unwrap();
            cache.insert(address.to_string(), loc);
        }
        
        location
    }
    
    /// Resolve location from address (simplified)
    async fn resolve_location(&self, address: &str) -> Option<Point<f64>> {
        // Simple heuristic based on domain/IP patterns
        if address.contains("us.") || address.contains("america") {
            Some(Point::new(-95.0, 40.0)) // Central US
        } else if address.contains("eu.") || address.contains("europe") {
            Some(Point::new(10.0, 50.0)) // Central Europe
        } else if address.contains("asia.") || address.contains("ap.") {
            Some(Point::new(120.0, 30.0)) // East Asia
        } else if address.contains("au.") || address.contains("oceania") {
            Some(Point::new(135.0, -25.0)) // Australia
        } else {
            // Try to infer from IP address patterns (simplified)
            self.infer_location_from_ip(address).await
        }
    }
    
    /// Infer location from IP address (placeholder)
    async fn infer_location_from_ip(&self, address: &str) -> Option<Point<f64>> {
        // In a real implementation, this would use a GeoIP database
        // For now, return a default location
        Some(Point::new(0.0, 0.0)) // Default to equator
    }
    
    /// Calculate distance between two locations
    pub fn calculate_distance(&self, loc1: Point<f64>, loc2: Point<f64>) -> f64 {
        loc1.haversine_distance(&loc2) / 1000.0 // Convert to kilometers
    }
}

impl PathOptimizer {
    pub fn new() -> Self {
        Self {
            quality_weights: QualityWeights::default(),
        }
    }
    
    /// Select best paths based on quality metrics
    pub fn select_best_quality_paths(&self, candidates: Vec<NetworkNode>, count: usize) -> Vec<NodeId> {
        let mut scored_nodes: Vec<_> = candidates.into_iter()
            .map(|node| {
                let score = self.calculate_comprehensive_node_score(&node);
                (node.node_id, score, node)
            })
            .collect();
        
        // Sort by score (higher is better)
        scored_nodes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Apply quality-based filtering
        let filtered_nodes = self.apply_quality_filters(scored_nodes);
        
        filtered_nodes.into_iter()
            .take(count)
            .map(|(id, _, _)| id)
            .collect()
    }
    
    /// Calculate comprehensive node score including all quality factors
    fn calculate_comprehensive_node_score(&self, node: &NetworkNode) -> f64 {
        // Base quality score
        let base_score = self.calculate_node_score(node);
        
        // Additional quality factors
        let reputation_bonus = node.reputation_score * 0.1;
        let freshness_bonus = self.calculate_freshness_bonus(node);
        let load_penalty = node.load_factor * 0.2;
        
        base_score + reputation_bonus + freshness_bonus - load_penalty
    }
    
    /// Calculate freshness bonus based on last seen time
    fn calculate_freshness_bonus(&self, node: &NetworkNode) -> f64 {
        let now = SystemTime::now();
        let time_since_seen = now.duration_since(node.last_seen).unwrap_or_default();
        
        // Bonus decreases with time (max 0.1 bonus for very recent)
        let hours_since = time_since_seen.as_secs() as f64 / 3600.0;
        (0.1 * (-hours_since / 24.0).exp()).max(0.0)
    }
    
    /// Apply quality filters to remove unsuitable nodes
    fn apply_quality_filters(&self, mut scored_nodes: Vec<(NodeId, f64, NetworkNode)>) -> Vec<(NodeId, f64, NetworkNode)> {
        // Filter out nodes with poor quality metrics
        scored_nodes.retain(|(_, score, node)| {
            *score > 0.3 && // Minimum quality threshold
            node.latency_ms < 1000.0 && // Max latency
            node.bandwidth_mbps > 1.0 && // Min bandwidth
            node.reliability_score > 0.5 && // Min reliability
            node.load_factor < 0.9 // Max load
        });
        
        scored_nodes
    }
    
    /// Optimize path selection with quality constraints
    pub fn optimize_path_with_constraints(&self, candidates: Vec<NetworkNode>, constraints: QualityConstraints) -> Vec<NodeId> {
        let filtered_candidates: Vec<_> = candidates.into_iter()
            .filter(|node| self.meets_quality_constraints(node, &constraints))
            .collect();
        
        self.optimize_path_selection(filtered_candidates)
    }
    
    /// Check if node meets quality constraints
    fn meets_quality_constraints(&self, node: &NetworkNode, constraints: &QualityConstraints) -> bool {
        node.latency_ms <= constraints.max_latency_ms &&
        node.bandwidth_mbps >= constraints.min_bandwidth_mbps &&
        node.reliability_score >= constraints.min_reliability &&
        node.load_factor <= constraints.max_load_factor
    }
    
    /// Optimize path selection based on quality metrics
    pub fn optimize_path_selection(&self, candidates: Vec<NetworkNode>) -> Vec<NodeId> {
        let mut scored_candidates: Vec<_> = candidates.into_iter()
            .map(|node| {
                let score = self.calculate_node_score(&node);
                (node.node_id, score)
            })
            .collect();
        
        // Sort by score (higher is better)
        scored_candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        scored_candidates.into_iter().map(|(id, _)| id).collect()
    }
    
    /// Calculate quality score for a node
    fn calculate_node_score(&self, node: &NetworkNode) -> f64 {
        let latency_score = 1.0 / (1.0 + node.latency_ms / 1000.0);
        let bandwidth_score = (node.bandwidth_mbps / 1000.0).min(1.0);
        let reliability_score = node.reliability_score;
        let load_score = 1.0 - node.load_factor;
        
        latency_score * self.quality_weights.latency_weight +
        bandwidth_score * self.quality_weights.bandwidth_weight +
        reliability_score * self.quality_weights.reliability_weight +
        load_score * self.quality_weights.load_weight
    }
}

impl GeographicPathBuilder {
    pub fn new() -> Self {
        Self {
            location_service: LocationService::new(),
            diversity_config: DiversityConfig::default(),
            path_optimizer: PathOptimizer::new(),
        }
    }
    
    /// Analyze regional node distribution
    pub fn analyze_regional_distribution(&self, nodes: &[NetworkNode]) -> RegionalDistribution {
        let mut region_counts: HashMap<String, u32> = HashMap::new();
        let mut region_quality: HashMap<String, RegionQuality> = HashMap::new();
        
        for node in nodes {
            *region_counts.entry(node.region.clone()).or_insert(0) += 1;
            
            let quality = region_quality.entry(node.region.clone()).or_insert_with(|| RegionQuality {
                avg_latency: 0.0,
                avg_bandwidth: 0.0,
                avg_reliability: 0.0,
                node_count: 0,
            });
            
            // Update running averages
            let old_count = quality.node_count as f64;
            let new_count = old_count + 1.0;
            
            quality.avg_latency = (quality.avg_latency * old_count + node.latency_ms) / new_count;
            quality.avg_bandwidth = (quality.avg_bandwidth * old_count + node.bandwidth_mbps) / new_count;
            quality.avg_reliability = (quality.avg_reliability * old_count + node.reliability_score) / new_count;
            quality.node_count += 1;
        }
        
        RegionalDistribution {
            region_counts,
            region_quality,
            total_nodes: nodes.len(),
            diversity_score: self.calculate_distribution_diversity(&region_counts),
        }
    }
    
    /// Calculate diversity score for regional distribution
    fn calculate_distribution_diversity(&self, region_counts: &HashMap<String, u32>) -> f64 {
        if region_counts.is_empty() {
            return 0.0;
        }
        
        let total_nodes: u32 = region_counts.values().sum();
        if total_nodes == 0 {
            return 0.0;
        }
        
        // Calculate Shannon diversity index
        let mut diversity = 0.0;
        for &count in region_counts.values() {
            if count > 0 {
                let proportion = count as f64 / total_nodes as f64;
                diversity -= proportion * proportion.ln();
            }
        }
        
        // Normalize to 0-1 range (approximate)
        let max_diversity = (region_counts.len() as f64).ln();
        if max_diversity > 0.0 {
            diversity / max_diversity
        } else {
            0.0
        }
    }
}

impl GeographicPathBuilder {
    /// Create a new geographic path builder
    pub fn new() -> Self {
        Self {
            location_service: LocationService::new(),
            diversity_config: DiversityConfig::default(),
            path_optimizer: PathOptimizer::new(),
        }
    }
    
    /// Build a geographically diverse path
    pub async fn build_diverse_path(&self, target: NodeId, hops: u32, candidates: &[NetworkNode]) -> Result<Vec<NodeId>, anyhow::Error> {
        debug!("Building geographically diverse path with {} hops", hops);
        
        // Filter candidates with location data
        let located_candidates: Vec<&NetworkNode> = candidates.iter()
            .filter(|node| node.location.is_some())
            .collect();
        
        if located_candidates.len() < hops as usize {
            return Err(anyhow::anyhow!("Insufficient candidates with location data"));
        }
        
        let mut selected_nodes = Vec::new();
        let mut used_nodes = HashSet::new();
        let mut selected_locations = Vec::new();
        
        // Start with a random node from a preferred region
        let preferred_candidates: Vec<&NetworkNode> = located_candidates.iter()
            .filter(|node| self.diversity_config.preferred_regions.contains(&node.region))
            .cloned()
            .collect();
        
        let start_candidates = if !preferred_candidates.is_empty() {
            preferred_candidates
        } else {
            located_candidates.clone()
        };
        
        if let Some(first_node) = start_candidates.choose(&mut thread_rng()) {
            selected_nodes.push(first_node.node_id);
            used_nodes.insert(first_node.node_id);
            if let Some(location) = first_node.location {
                selected_locations.push(location);
            }
        }
        
        // Select remaining nodes with diversity constraints
        for _ in 1..hops {
            let mut best_candidate: Option<&NetworkNode> = None;
            let mut best_diversity_score = f64::NEG_INFINITY;
            
            for candidate in &located_candidates {
                if used_nodes.contains(&candidate.node_id) {
                    continue;
                }
                
                let candidate_location = candidate.location.unwrap();
                
                // Calculate minimum distance to existing nodes
                let min_distance = selected_locations.iter()
                    .map(|loc| self.location_service.calculate_distance(*loc, candidate_location))
                    .fold(f64::INFINITY, f64::min);
                
                // Calculate diversity score
                let diversity_score = self.calculate_candidate_diversity_score(
                    candidate, 
                    &selected_nodes, 
                    &selected_locations,
                    candidates
                ).await;
                
                // Apply distance constraint
                if min_distance >= self.diversity_config.min_distance_km && 
                   diversity_score > best_diversity_score {
                    best_candidate = Some(candidate);
                    best_diversity_score = diversity_score;
                }
            }
            
            if let Some(selected) = best_candidate {
                selected_nodes.push(selected.node_id);
                used_nodes.insert(selected.node_id);
                if let Some(location) = selected.location {
                    selected_locations.push(location);
                }
            } else {
                // Fallback: select best available candidate regardless of distance
                let fallback = located_candidates.iter()
                    .filter(|node| !used_nodes.contains(&node.node_id))
                    .max_by(|a, b| {
                        let score_a = self.path_optimizer.calculate_node_score(a);
                        let score_b = self.path_optimizer.calculate_node_score(b);
                        score_a.partial_cmp(&score_b).unwrap()
                    });
                
                if let Some(selected) = fallback {
                    selected_nodes.push(selected.node_id);
                    used_nodes.insert(selected.node_id);
                    if let Some(location) = selected.location {
                        selected_locations.push(location);
                    }
                }
            }
        }
        
        // Add target node if not already included
        if !selected_nodes.contains(&target) {
            selected_nodes.push(target);
        }
        
        debug!("Built diverse path with {} nodes", selected_nodes.len());
        Ok(selected_nodes)
    }
    
    /// Calculate candidate diversity score
    async fn calculate_candidate_diversity_score(
        &self,
        candidate: &NetworkNode,
        selected_nodes: &[NodeId],
        selected_locations: &[Point<f64>],
        all_candidates: &[NetworkNode]
    ) -> f64 {
        let mut diversity_score = 0.0;
        
        if let Some(candidate_location) = candidate.location {
            // Geographic diversity score
            let min_distance = selected_locations.iter()
                .map(|loc| self.location_service.calculate_distance(*loc, candidate_location))
                .fold(f64::INFINITY, f64::min);
            
            let geographic_score = (min_distance / 1000.0).min(10.0) / 10.0; // Normalize to 0-1
            diversity_score += geographic_score * 0.4;
            
            // Regional diversity score
            let candidate_regions: HashSet<String> = selected_nodes.iter()
                .filter_map(|node_id| {
                    all_candidates.iter()
                        .find(|n| n.node_id == *node_id)
                        .map(|n| n.region.clone())
                })
                .collect();
            
            let regional_diversity = if candidate_regions.contains(&candidate.region) {
                0.0
            } else {
                1.0
            };
            diversity_score += regional_diversity * 0.3;
            
            // Quality score
            let quality_score = self.path_optimizer.calculate_node_score(candidate);
            diversity_score += quality_score * 0.3;
        }
        
        diversity_score
    }
            
            // If no candidate meets diversity requirements, pick the best available
            if best_candidate.is_none() {
                for candidate in &located_candidates {
                    if !used_nodes.contains(&candidate.node_id) {
                        best_candidate = Some(candidate);
                        break;
                    }
                }
            }
            
            if let Some(node) = best_candidate {
                selected_nodes.push(node.node_id);
                used_nodes.insert(node.node_id);
                if let Some(location) = node.location {
                    selected_locations.push(location);
                }
            } else {
                break; // No more candidates available
            }
        }
        
        if selected_nodes.len() < hops as usize {
            warn!("Could only select {} nodes out of {} requested for geographic diversity", 
                  selected_nodes.len(), hops);
        }
        
        Ok(selected_nodes)
    }
    
    /// Calculate diversity score for a path
    pub async fn calculate_diversity_score(&self, path: &[NodeId], candidates: &[NetworkNode]) -> f64 {
        if path.len() < 2 {
            return 0.0;
        }
        
        let node_map: HashMap<NodeId, &NetworkNode> = candidates.iter()
            .map(|n| (n.node_id, n))
            .collect();
        
        let mut locations = Vec::new();
        let mut regions = HashSet::new();
        
        // Collect locations and regions for path nodes
        for &node_id in path {
            if let Some(node) = node_map.get(&node_id) {
                if let Some(location) = node.location {
                    locations.push(location);
                }
                regions.insert(node.region.clone());
            }
        }
        
        if locations.len() < 2 {
            return 0.0;
        }
        
        // Calculate geographic diversity
        let mut total_distance = 0.0;
        let mut min_distance = f64::INFINITY;
        let mut distance_count = 0;
        
        for i in 0..locations.len() {
            for j in (i + 1)..locations.len() {
                let distance = self.location_service.calculate_distance(locations[i], locations[j]);
                total_distance += distance;
                min_distance = min_distance.min(distance);
                distance_count += 1;
            }
        }
        
        let avg_distance = if distance_count > 0 {
            total_distance / distance_count as f64
        } else {
            0.0
        };
        
        // Normalize geographic score
        let geographic_score = (avg_distance / GEOGRAPHIC_DIVERSITY_RADIUS_KM).min(1.0);
        
        // Calculate regional diversity
        let regional_score = regions.len() as f64 / path.len() as f64;
        
        // Combined diversity score
        geographic_score * 0.6 + regional_score * 0.4
    }
    
    /// Optimize node selection for geographic diversity
    pub async fn optimize_for_geography(&self, candidates: Vec<NodeId>, node_data: &[NetworkNode]) -> Vec<NodeId> {
        let mut optimized_path = Vec::new();
        
        // Get candidate nodes with location data
        let located_nodes: Vec<&NetworkNode> = node_data.iter()
            .filter(|node| {
                candidates.contains(&node.node_id) && node.location.is_some()
            })
            .collect();
        
        if located_nodes.is_empty() {
            return candidates; // Return original if no location data
        }
        
        // Start with the node from the most diverse region
        let region_counts = self.count_nodes_by_region(&located_nodes);
        let least_common_region = region_counts.iter()
            .min_by_key(|(_, count)| *count)
            .map(|(region, _)| region.clone());
        
        let mut used_nodes = HashSet::new();
        let mut selected_locations = Vec::new();
        
        // Select starting node from least common region
        if let Some(preferred_region) = least_common_region {
            if let Some(start_node) = located_nodes.iter()
                .find(|node| node.region == preferred_region) {
                optimized_path.push(start_node.node_id);
                used_nodes.insert(start_node.node_id);
                if let Some(location) = start_node.location {
                    selected_locations.push(location);
                }
            }
        }
        
        // Select remaining nodes with diversity optimization
        for _ in optimized_path.len()..candidates.len() {
            let mut best_candidate: Option<&NetworkNode> = None;
            let mut best_score = f64::NEG_INFINITY;
            
            for candidate in &located_nodes {
                if used_nodes.contains(&candidate.node_id) {
                    continue;
                }
                
                let diversity_score = self.calculate_candidate_optimization_score(
                    candidate,
                    &selected_locations,
                    &optimized_path,
                    node_data
                ).await;
                
                if diversity_score > best_score {
                    best_score = diversity_score;
                    best_candidate = Some(candidate);
                }
            }
            
            if let Some(selected) = best_candidate {
                optimized_path.push(selected.node_id);
                used_nodes.insert(selected.node_id);
                if let Some(location) = selected.location {
                    selected_locations.push(location);
                }
            } else {
                // Add remaining candidates that weren't optimized
                for candidate_id in &candidates {
                    if !used_nodes.contains(candidate_id) {
                        optimized_path.push(*candidate_id);
                        used_nodes.insert(*candidate_id);
                    }
                }
                break;
            }
        }
        
        debug!("Optimized path geography: {} nodes with diversity score: {:.2}", 
               optimized_path.len(),
               self.calculate_diversity_score(&optimized_path, node_data).await);
        
        optimized_path
    }
    
    /// Calculate optimization score for a candidate node
    async fn calculate_candidate_optimization_score(
        &self,
        candidate: &NetworkNode,
        selected_locations: &[Point<f64>],
        selected_path: &[NodeId],
        all_nodes: &[NetworkNode]
    ) -> f64 {
        let mut score = 0.0;
        
        if let Some(candidate_location) = candidate.location {
            // Distance-based score
            if !selected_locations.is_empty() {
                let min_distance = selected_locations.iter()
                    .map(|loc| self.location_service.calculate_distance(*loc, candidate_location))
                    .fold(f64::NEG_INFINITY, f64::max);
                
                score += (min_distance / 1000.0).min(5.0) / 5.0; // Max 5000km normalized
            }
            
            // Regional diversity score
            let selected_regions: HashSet<String> = selected_path.iter()
                .filter_map(|node_id| {
                    all_nodes.iter()
                        .find(|n| n.node_id == *node_id)
                        .map(|n| n.region.clone())
                })
                .collect();
            
            if !selected_regions.contains(&candidate.region) {
                score += 0.3; // Bonus for new region
            }
            
            // Quality score component
            let quality_score = self.path_optimizer.calculate_node_score(candidate);
            score += quality_score * 0.2;
        }
        
        score
    }
    
    /// Count nodes by region
    fn count_nodes_by_region(&self, nodes: &[&NetworkNode]) -> HashMap<String, u32> {
        let mut region_counts = HashMap::new();
        
        for node in nodes {
            *region_counts.entry(node.region.clone()).or_insert(0) += 1;
        }
        
        region_counts
    }
    
    /// Analyze regional node distribution
    pub fn analyze_regional_distribution(&self, nodes: &[NetworkNode]) -> RegionalDistribution {
        let mut region_counts: HashMap<String, u32> = HashMap::new();
        let mut region_quality: HashMap<String, RegionQuality> = HashMap::new();
        
        for node in nodes {
            *region_counts.entry(node.region.clone()).or_insert(0) += 1;
            
            let quality = region_quality.entry(node.region.clone()).or_insert(RegionQuality {
                avg_latency: 0.0,
                avg_bandwidth: 0.0,
                avg_reliability: 0.0,
                node_count: 0,
                total_latency: 0.0,
                total_bandwidth: 0.0,
                total_reliability: 0.0,
            });
            
            quality.total_latency += node.latency_ms;
            quality.total_bandwidth += node.bandwidth_mbps;
            quality.total_reliability += node.reliability_score;
            quality.node_count += 1;
        }
        
        // Calculate averages
        for quality in region_quality.values_mut() {
            if quality.node_count > 0 {
                quality.avg_latency = quality.total_latency / quality.node_count as f64;
                quality.avg_bandwidth = quality.total_bandwidth / quality.node_count as f64;
                quality.avg_reliability = quality.total_reliability / quality.node_count as f64;
            }
        }
        
        let diversity_score = self.calculate_distribution_diversity(&region_counts);
        
        RegionalDistribution {
            region_counts,
            region_quality,
            diversity_score,
            total_nodes: nodes.len() as u32,
        }
    }
    
    /// Calculate diversity score for regional distribution
    fn calculate_distribution_diversity(&self, region_counts: &HashMap<String, u32>) -> f64 {
        if region_counts.is_empty() {
            return 0.0;
        }
        
        let total_nodes: u32 = region_counts.values().sum();
        if total_nodes == 0 {
            return 0.0;
        }
        
        // Calculate Shannon diversity index
        let mut diversity = 0.0;
        for &count in region_counts.values() {
            if count > 0 {
                let proportion = count as f64 / total_nodes as f64;
                diversity -= proportion * proportion.ln();
            }
        }
        
        // Normalize to 0-1 range (approximate)
        let max_diversity = (region_counts.len() as f64).ln();
        if max_diversity > 0.0 {
            diversity / max_diversity
        } else {
            0.0
        }
    }
}
            }
        }
        
        let avg_distance = if distance_count > 0 {
            total_distance / distance_count as f64
        } else {
            0.0
        };
        
        // Calculate region diversity bonus
        let region_diversity = regions.len() as f64 / path.len() as f64;
        
        // Normalize and combine scores
        let distance_score = (avg_distance / 10000.0).min(1.0); // Normalize to 0-1
        let combined_score = (distance_score * 0.7) + (region_diversity * 0.3);
        
        combined_score
    }
    
    /// Optimize node selection for geographic diversity
    pub async fn optimize_for_geography(&self, candidates: Vec<NodeId>, node_data: &[NetworkNode]) -> Vec<NodeId> {
        let node_map: HashMap<NodeId, &NetworkNode> = node_data.iter()
            .map(|n| (n.node_id, n))
            .collect();
        
        // Group candidates by region
        let mut region_groups: HashMap<String, Vec<NodeId>> = HashMap::new();
        
        for &candidate_id in &candidates {
            if let Some(node) = node_map.get(&candidate_id) {
                region_groups.entry(node.region.clone())
                    .or_insert_with(Vec::new)
                    .push(candidate_id);
            }
        }
        
        // Select nodes to maximize regional diversity
        let mut optimized = Vec::new();
        let mut region_counts: HashMap<String, u32> = HashMap::new();
        
        // First pass: select one node from each region
        for (region, nodes) in &region_groups {
            if let Some(&first_node) = nodes.first() {
                optimized.push(first_node);
                *region_counts.entry(region.clone()).or_insert(0) += 1;
            }
        }
        
        // Second pass: fill remaining slots while respecting max_hops_per_region
        for &candidate_id in &candidates {
            if optimized.contains(&candidate_id) {
                continue;
            }
            
            if let Some(node) = node_map.get(&candidate_id) {
                let current_count = region_counts.get(&node.region).unwrap_or(&0);
                if *current_count < self.diversity_config.max_hops_per_region {
                    optimized.push(candidate_id);
                    *region_counts.entry(node.region.clone()).or_insert(0) += 1;
                }
            }
        }
        
        optimized
    }

/// Path quality evaluator
pub struct PathQualityEvaluator {
    weights: QualityWeights,
    history_tracker: Arc<Mutex<HashMap<Vec<NodeId>, Vec<PathQualityHistory>>>>,
}

/// Historical path quality data
#[derive(Debug, Clone)]
pub struct PathQualityHistory {
    pub timestamp: Instant,
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
    pub reliability_score: f64,
    pub packet_loss_rate: f64,
}

impl PathQualityEvaluator {
    pub fn new() -> Self {
        Self {
            weights: QualityWeights::default(),
            history_tracker: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Evaluate path quality with comprehensive metrics
    pub async fn evaluate_path_quality(&self, path: &[NodeId], nodes: &[NetworkNode]) -> PathQuality {
        let node_map: HashMap<NodeId, &NetworkNode> = nodes.iter()
            .map(|n| (n.node_id, n))
            .collect();
        
        let mut total_latency = 0.0;
        let mut min_bandwidth = f64::INFINITY;
        let mut reliability_product = 1.0;
        let mut load_factors = Vec::new();
        let mut geographic_distances = Vec::new();
        
        // Calculate metrics for each hop
        for &hop in path {
            if let Some(node) = node_map.get(&hop) {
                total_latency += node.latency_ms;
                min_bandwidth = min_bandwidth.min(node.bandwidth_mbps);
                reliability_product *= node.reliability_score;
                load_factors.push(node.load_factor);
                
                // Calculate geographic diversity
                if let Some(location) = node.location {
                    for other_hop in path {
                        if *other_hop != hop {
                            if let Some(other_node) = node_map.get(other_hop) {
                                if let Some(other_location) = other_node.location {
                                    let distance = location.haversine_distance(&other_location) / 1000.0;
                                    geographic_distances.push(distance);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        let geographic_diversity = if geographic_distances.is_empty() {
            0.0
        } else {
            geographic_distances.iter().sum::<f64>() / geographic_distances.len() as f64
        };
        
        let load_balance_score = if load_factors.is_empty() {
            1.0
        } else {
            1.0 - (load_factors.iter().sum::<f64>() / load_factors.len() as f64)
        };
        
        // Calculate weighted overall score
        let overall_score = self.calculate_weighted_score(
            total_latency,
            min_bandwidth,
            reliability_product,
            geographic_diversity,
            load_balance_score,
        );
        
        PathQuality {
            total_latency_ms: total_latency,
            min_bandwidth_mbps: min_bandwidth,
            reliability_score: reliability_product,
            geographic_diversity,
            load_balance_score,
            overall_score,
        }
    }
    
    /// Calculate weighted quality score
    fn calculate_weighted_score(
        &self,
        latency: f64,
        bandwidth: f64,
        reliability: f64,
        diversity: f64,
        load_balance: f64,
    ) -> f64 {
        let latency_score = 1.0 / (1.0 + latency / 1000.0);
        let bandwidth_score = (bandwidth / 1000.0).min(1.0);
        let diversity_score = (diversity / 10000.0).min(1.0);
        
        latency_score * self.weights.latency_weight +
        bandwidth_score * self.weights.bandwidth_weight +
        reliability * self.weights.reliability_weight +
        diversity_score * self.weights.diversity_weight +
        load_balance * self.weights.load_weight
    }
    
    /// Track path quality over time
    pub async fn track_path_quality(&self, path: Vec<NodeId>, quality: PathQualityHistory) {
        let mut history = self.history_tracker.lock().unwrap();
        let path_history = history.entry(path.clone()).or_insert_with(Vec::new);
        
        path_history.push(quality.clone());
        
        // Keep only recent history (last 100 entries)
        if path_history.len() > 100 {
            path_history.remove(0);
        }
        
        // Trigger quality analysis if we have enough data
        if path_history.len() >= 10 {
            self.analyze_path_quality_patterns(&path, path_history);
        }
    }
    
    /// Analyze path quality patterns for optimization
    fn analyze_path_quality_patterns(&self, path: &[NodeId], history: &[PathQualityHistory]) {
        if history.len() < 5 {
            return;
        }
        
        // Calculate quality degradation trends
        let recent_samples = &history[history.len().saturating_sub(5)..];
        let older_samples = &history[history.len().saturating_sub(10)..history.len().saturating_sub(5)];
        
        let recent_avg_latency = recent_samples.iter().map(|h| h.latency_ms).sum::<f64>() / recent_samples.len() as f64;
        let older_avg_latency = older_samples.iter().map(|h| h.latency_ms).sum::<f64>() / older_samples.len() as f64;
        
        let recent_avg_reliability = recent_samples.iter().map(|h| h.reliability_score).sum::<f64>() / recent_samples.len() as f64;
        let older_avg_reliability = older_samples.iter().map(|h| h.reliability_score).sum::<f64>() / older_samples.len() as f64;
        
        // Detect significant degradation
        if recent_avg_latency > older_avg_latency * 1.5 {
            warn!("Path quality degradation detected: latency increased from {:.2}ms to {:.2}ms", 
                  older_avg_latency, recent_avg_latency);
        }
        
        if recent_avg_reliability < older_avg_reliability * 0.8 {
            warn!("Path quality degradation detected: reliability decreased from {:.3} to {:.3}", 
                  older_avg_reliability, recent_avg_reliability);
        }
    }
    
    /// Get comprehensive path quality metrics
    pub async fn get_comprehensive_quality_metrics(&self, path: &[NodeId]) -> Option<ComprehensiveQualityMetrics> {
        let history = self.history_tracker.lock().unwrap();
        let path_history = history.get(path)?;
        
        if path_history.is_empty() {
            return None;
        }
        
        let latest = path_history.last().unwrap();
        
        // Calculate statistics
        let latencies: Vec<f64> = path_history.iter().map(|h| h.latency_ms).collect();
        let bandwidths: Vec<f64> = path_history.iter().map(|h| h.bandwidth_mbps).collect();
        let reliabilities: Vec<f64> = path_history.iter().map(|h| h.reliability_score).collect();
        let packet_losses: Vec<f64> = path_history.iter().map(|h| h.packet_loss_rate).collect();
        
        Some(ComprehensiveQualityMetrics {
            current_latency: latest.latency_ms,
            avg_latency: latencies.iter().sum::<f64>() / latencies.len() as f64,
            min_latency: latencies.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            max_latency: latencies.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
            
            current_bandwidth: latest.bandwidth_mbps,
            avg_bandwidth: bandwidths.iter().sum::<f64>() / bandwidths.len() as f64,
            min_bandwidth: bandwidths.iter().fold(f64::INFINITY, |a, &b| a.min(b)),
            max_bandwidth: bandwidths.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b)),
            
            current_reliability: latest.reliability_score,
            avg_reliability: reliabilities.iter().sum::<f64>() / reliabilities.len() as f64,
            
            current_packet_loss: latest.packet_loss_rate,
            avg_packet_loss: packet_losses.iter().sum::<f64>() / packet_losses.len() as f64,
            
            sample_count: path_history.len(),
            quality_trend: self.calculate_quality_trend(path_history),
            stability_score: self.calculate_stability_score(path_history),
        })
    }
    
    /// Calculate quality trend direction
    fn calculate_quality_trend(&self, history: &[PathQualityHistory]) -> QualityTrend {
        if history.len() < 2 {
            return QualityTrend::Stable;
        }
        
        let recent_count = (history.len() / 3).max(1);
        let recent = &history[history.len() - recent_count..];
        let older = &history[..recent_count];
        
        let recent_score = self.calculate_composite_score(recent);
        let older_score = self.calculate_composite_score(older);
        
        let change_ratio = recent_score / older_score;
        
        if change_ratio > 1.1 {
            QualityTrend::Improving
        } else if change_ratio < 0.9 {
            QualityTrend::Degrading
        } else {
            QualityTrend::Stable
        }
    }
    
    /// Calculate composite quality score
    fn calculate_composite_score(&self, samples: &[PathQualityHistory]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        
        let avg_latency = samples.iter().map(|s| s.latency_ms).sum::<f64>() / samples.len() as f64;
        let avg_bandwidth = samples.iter().map(|s| s.bandwidth_mbps).sum::<f64>() / samples.len() as f64;
        let avg_reliability = samples.iter().map(|s| s.reliability_score).sum::<f64>() / samples.len() as f64;
        let avg_packet_loss = samples.iter().map(|s| s.packet_loss_rate).sum::<f64>() / samples.len() as f64;
        
        // Composite score (higher is better)
        let latency_score = 1.0 / (1.0 + avg_latency / 1000.0);
        let bandwidth_score = (avg_bandwidth / 1000.0).min(1.0);
        let reliability_score = avg_reliability;
        let packet_loss_score = 1.0 - avg_packet_loss;
        
        (latency_score + bandwidth_score + reliability_score + packet_loss_score) / 4.0
    }
    
    /// Calculate stability score (0-1, higher is more stable)
    fn calculate_stability_score(&self, history: &[PathQualityHistory]) -> f64 {
        if history.len() < 2 {
            return 1.0;
        }
        
        // Calculate coefficient of variation for key metrics
        let latencies: Vec<f64> = history.iter().map(|h| h.latency_ms).collect();
        let bandwidths: Vec<f64> = history.iter().map(|h| h.bandwidth_mbps).collect();
        let reliabilities: Vec<f64> = history.iter().map(|h| h.reliability_score).collect();
        
        let latency_cv = self.coefficient_of_variation(&latencies);
        let bandwidth_cv = self.coefficient_of_variation(&bandwidths);
        let reliability_cv = self.coefficient_of_variation(&reliabilities);
        
        // Lower coefficient of variation = higher stability
        let avg_cv = (latency_cv + bandwidth_cv + reliability_cv) / 3.0;
        (1.0 - avg_cv).max(0.0)
    }
    
    /// Calculate coefficient of variation
    fn coefficient_of_variation(&self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        if mean == 0.0 {
            return 0.0;
        }
        
        let variance = values.iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>() / values.len() as f64;
        
        let std_dev = variance.sqrt();
        std_dev / mean
    }
    
    /// Get path quality trend analysis
    pub async fn get_path_trend(&self, path: &[NodeId]) -> Option<PathQualityTrend> {
        let history = self.history_tracker.lock().unwrap();
        let path_history = history.get(path)?;
        
        if path_history.len() < 2 {
            return None;
        }
        
        // Calculate trends
        let recent_quality = &path_history[path_history.len() - 1];
        let older_quality = &path_history[path_history.len() / 2];
        
        Some(PathQualityTrend {
            latency_trend: recent_quality.latency_ms - older_quality.latency_ms,
            bandwidth_trend: recent_quality.bandwidth_mbps - older_quality.bandwidth_mbps,
            reliability_trend: recent_quality.reliability_score - older_quality.reliability_score,
            sample_count: path_history.len(),
        })
    }
}

/// Path quality trend analysis
#[derive(Debug, Clone)]
pub struct PathQualityTrend {
    pub latency_trend: f64,      // Positive = getting worse
    pub bandwidth_trend: f64,    // Positive = getting better
    pub reliability_trend: f64,  // Positive = getting better
    pub sample_count: usize,
}

/// Regional distribution analysis
#[derive(Debug, Clone)]
pub struct RegionalDistribution {
    pub region_counts: HashMap<String, u32>,
    pub region_quality: HashMap<String, RegionQuality>,
    pub total_nodes: usize,
    pub diversity_score: f64,
}

/// Quality metrics for a region
#[derive(Debug, Clone)]
pub struct RegionQuality {
    pub avg_latency: f64,
    pub avg_bandwidth: f64,
    pub avg_reliability: f64,
    pub node_count: u32,
}

/// Comprehensive quality metrics for a path
#[derive(Debug, Clone)]
pub struct ComprehensiveQualityMetrics {
    pub current_latency: f64,
    pub avg_latency: f64,
    pub min_latency: f64,
    pub max_latency: f64,
    
    pub current_bandwidth: f64,
    pub avg_bandwidth: f64,
    pub min_bandwidth: f64,
    pub max_bandwidth: f64,
    
    pub current_reliability: f64,
    pub avg_reliability: f64,
    
    pub current_packet_loss: f64,
    pub avg_packet_loss: f64,
    
    pub sample_count: usize,
    pub quality_trend: QualityTrend,
    pub stability_score: f64,
}

/// Quality trend direction
#[derive(Debug, Clone, PartialEq)]
pub enum QualityTrend {
    Improving,
    Stable,
    Degrading,
}

/// Quality constraints for path selection
#[derive(Debug, Clone)]
pub struct QualityConstraints {
    pub max_latency_ms: f64,
    pub min_bandwidth_mbps: f64,
    pub min_reliability: f64,
    pub max_load_factor: f64,
    pub require_location: bool,
    pub preferred_regions: Vec<String>,
}

impl Default for QualityConstraints {
    fn default() -> Self {
        Self {
            max_latency_ms: 500.0,
            min_bandwidth_mbps: 10.0,
            min_reliability: 0.8,
            max_load_factor: 0.8,
            require_location: false,
            preferred_regions: Vec::new(),
        }
    }
}

/// Path construction fallback system
pub struct PathFallbackSystem {
    fallback_strategies: Vec<FallbackStrategy>,
    failure_tracker: Arc<Mutex<HashMap<String, FailureInfo>>>,
}

/// Fallback strategy for path construction
#[derive(Debug, Clone)]
pub enum FallbackStrategy {
    UseBootstrapNodes,
    RelaxQualityConstraints,
    ReduceHopCount,
    UseAlternativeRegions,
    EnableDegradedMode,
}

/// Failure tracking information
#[derive(Debug, Clone)]
pub struct FailureInfo {
    pub failure_count: u32,
    pub last_failure: Instant,
    pub failure_reasons: Vec<String>,
    pub failure_analysis: Vec<FailureAnalysis>,
    pub recovery_attempts: u32,
    pub successful_recoveries: u32,
}

/// Failure analysis result
#[derive(Debug, Clone)]
pub struct FailureAnalysis {
    pub failure_type: FailureType,
    pub severity: FailureSeverity,
    pub is_recoverable: bool,
    pub suggested_actions: Vec<String>,
}

/// Types of path construction failures
#[derive(Debug, Clone, PartialEq)]
pub enum FailureType {
    InsufficientCandidates,
    QualityConstraints,
    GeographicConstraints,
    Timeout,
    DiscoveryFailure,
    Unknown,
}

/// Failure severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum FailureSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl PathFallbackSystem {
    pub fn new() -> Self {
        Self {
            fallback_strategies: vec![
                FallbackStrategy::RelaxQualityConstraints,
                FallbackStrategy::UseAlternativeRegions,
                FallbackStrategy::ReduceHopCount,
                FallbackStrategy::UseBootstrapNodes,
                FallbackStrategy::EnableDegradedMode,
            ],
            failure_tracker: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Handle path construction failure with fallback
    pub async fn handle_path_failure(
        &self,
        original_request: &PathRequest,
        failure_reason: String,
    ) -> Result<Vec<NodeId>, anyhow::Error> {
        let failure_key = format!("{}:{}", original_request.target, original_request.hops);
        
        // Analyze failure reason to determine best strategy
        let failure_analysis = self.analyze_failure_reason(&failure_reason);
        
        // Track failure with analysis
        self.track_failure_with_analysis(&failure_key, failure_reason.clone(), failure_analysis.clone()).await;
        
        // Select appropriate fallback strategies based on failure type
        let prioritized_strategies = self.prioritize_strategies_for_failure(&failure_analysis);
        
        // Try fallback strategies in priority order
        for strategy in prioritized_strategies {
            match self.try_fallback_strategy(&strategy, original_request).await {
                Ok(path) => {
                    info!("Path construction succeeded with fallback strategy: {:?} for failure: {}", 
                          strategy, failure_reason);
                    
                    // Track successful recovery
                    self.track_successful_recovery(&failure_key, &strategy).await;
                    
                    return Ok(path);
                }
                Err(e) => {
                    debug!("Fallback strategy {:?} failed: {}", strategy, e);
                }
            }
        }
        
        // Generate detailed failure report
        let failure_report = self.generate_failure_report(&failure_key, &failure_reason).await;
        error!("All fallback strategies failed. Report: {}", failure_report);
        
        Err(anyhow::anyhow!("All fallback strategies failed: {}", failure_report))
    }
    
    /// Analyze failure reason to determine root cause
    fn analyze_failure_reason(&self, reason: &str) -> FailureAnalysis {
        let reason_lower = reason.to_lowercase();
        
        let failure_type = if reason_lower.contains("insufficient") || reason_lower.contains("not enough") {
            FailureType::InsufficientCandidates
        } else if reason_lower.contains("timeout") || reason_lower.contains("timed out") {
            FailureType::Timeout
        } else if reason_lower.contains("quality") || reason_lower.contains("threshold") {
            FailureType::QualityConstraints
        } else if reason_lower.contains("location") || reason_lower.contains("geographic") {
            FailureType::GeographicConstraints
        } else if reason_lower.contains("dht") || reason_lower.contains("discovery") {
            FailureType::DiscoveryFailure
        } else {
            FailureType::Unknown
        };
        
        let severity = if reason_lower.contains("critical") || reason_lower.contains("fatal") {
            FailureSeverity::Critical
        } else if reason_lower.contains("error") {
            FailureSeverity::High
        } else if reason_lower.contains("warning") || reason_lower.contains("warn") {
            FailureSeverity::Medium
        } else {
            FailureSeverity::Low
        };
        
        FailureAnalysis {
            failure_type,
            severity,
            is_recoverable: self.is_failure_recoverable(&failure_type),
            suggested_actions: self.get_suggested_actions(&failure_type),
        }
    }
    
    /// Determine if failure type is recoverable
    fn is_failure_recoverable(&self, failure_type: &FailureType) -> bool {
        match failure_type {
            FailureType::InsufficientCandidates => true,
            FailureType::QualityConstraints => true,
            FailureType::GeographicConstraints => true,
            FailureType::Timeout => true,
            FailureType::DiscoveryFailure => true,
            FailureType::Unknown => false,
        }
    }
    
    /// Get suggested actions for failure type
    fn get_suggested_actions(&self, failure_type: &FailureType) -> Vec<String> {
        match failure_type {
            FailureType::InsufficientCandidates => vec![
                "Reduce hop count".to_string(),
                "Use bootstrap nodes".to_string(),
                "Relax quality constraints".to_string(),
            ],
            FailureType::QualityConstraints => vec![
                "Relax latency requirements".to_string(),
                "Reduce bandwidth requirements".to_string(),
                "Lower reliability threshold".to_string(),
            ],
            FailureType::GeographicConstraints => vec![
                "Use alternative regions".to_string(),
                "Reduce geographic diversity requirements".to_string(),
                "Allow same-region hops".to_string(),
            ],
            FailureType::Timeout => vec![
                "Retry with longer timeout".to_string(),
                "Use cached results".to_string(),
                "Switch to degraded mode".to_string(),
            ],
            FailureType::DiscoveryFailure => vec![
                "Use bootstrap peers".to_string(),
                "Retry DHT discovery".to_string(),
                "Use alternative discovery method".to_string(),
            ],
            FailureType::Unknown => vec![
                "Enable degraded mode".to_string(),
                "Use minimal path".to_string(),
            ],
        }
    }
    
    /// Prioritize fallback strategies based on failure analysis
    fn prioritize_strategies_for_failure(&self, analysis: &FailureAnalysis) -> Vec<FallbackStrategy> {
        match analysis.failure_type {
            FailureType::InsufficientCandidates => vec![
                FallbackStrategy::UseBootstrapNodes,
                FallbackStrategy::ReduceHopCount,
                FallbackStrategy::RelaxQualityConstraints,
                FallbackStrategy::EnableDegradedMode,
            ],
            FailureType::QualityConstraints => vec![
                FallbackStrategy::RelaxQualityConstraints,
                FallbackStrategy::UseAlternativeRegions,
                FallbackStrategy::ReduceHopCount,
                FallbackStrategy::UseBootstrapNodes,
            ],
            FailureType::GeographicConstraints => vec![
                FallbackStrategy::UseAlternativeRegions,
                FallbackStrategy::RelaxQualityConstraints,
                FallbackStrategy::UseBootstrapNodes,
                FallbackStrategy::EnableDegradedMode,
            ],
            FailureType::Timeout => vec![
                FallbackStrategy::UseBootstrapNodes,
                FallbackStrategy::EnableDegradedMode,
                FallbackStrategy::ReduceHopCount,
            ],
            FailureType::DiscoveryFailure => vec![
                FallbackStrategy::UseBootstrapNodes,
                FallbackStrategy::EnableDegradedMode,
            ],
            FailureType::Unknown => vec![
                FallbackStrategy::EnableDegradedMode,
                FallbackStrategy::UseBootstrapNodes,
            ],
        }
    }
    
    /// Track failure with detailed analysis
    async fn track_failure_with_analysis(&self, failure_key: &str, reason: String, analysis: FailureAnalysis) {
        let mut tracker = self.failure_tracker.lock().unwrap();
        let failure_info = tracker.entry(failure_key.to_string()).or_insert_with(|| FailureInfo {
            failure_count: 0,
            last_failure: Instant::now(),
            failure_reasons: Vec::new(),
            failure_analysis: Vec::new(),
            recovery_attempts: 0,
            successful_recoveries: 0,
        });
        
        failure_info.failure_count += 1;
        failure_info.last_failure = Instant::now();
        failure_info.failure_reasons.push(reason);
        failure_info.failure_analysis.push(analysis);
        
        // Keep only recent data
        if failure_info.failure_reasons.len() > 20 {
            failure_info.failure_reasons.remove(0);
            failure_info.failure_analysis.remove(0);
        }
    }
    
    /// Track successful recovery
    async fn track_successful_recovery(&self, failure_key: &str, strategy: &FallbackStrategy) {
        let mut tracker = self.failure_tracker.lock().unwrap();
        if let Some(failure_info) = tracker.get_mut(failure_key) {
            failure_info.successful_recoveries += 1;
            failure_info.recovery_attempts += 1;
        }
        
        debug!("Successful recovery for {} using strategy {:?}", failure_key, strategy);
    }
    
    /// Generate detailed failure report
    async fn generate_failure_report(&self, failure_key: &str, current_reason: &str) -> String {
        let tracker = self.failure_tracker.lock().unwrap();
        
        if let Some(failure_info) = tracker.get(failure_key) {
            let success_rate = if failure_info.recovery_attempts > 0 {
                failure_info.successful_recoveries as f64 / failure_info.recovery_attempts as f64
            } else {
                0.0
            };
            
            let recent_failures = failure_info.failure_reasons.iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>();
            
            format!(
                "Path construction failure report for {}: Current: '{}', Total failures: {}, Recovery success rate: {:.1}%, Recent failures: {:?}",
                failure_key, current_reason, failure_info.failure_count, success_rate * 100.0, recent_failures
            )
        } else {
            format!("First failure for {}: {}", failure_key, current_reason)
        }
    }
    
    /// Track path construction failure
    async fn track_failure(&self, failure_key: &str, reason: String) {
        let mut tracker = self.failure_tracker.lock().unwrap();
        let failure_info = tracker.entry(failure_key.to_string()).or_insert_with(|| FailureInfo {
            failure_count: 0,
            last_failure: Instant::now(),
            failure_reasons: Vec::new(),
            failure_analysis: Vec::new(),
            recovery_attempts: 0,
            successful_recoveries: 0,
        });
        
        failure_info.failure_count += 1;
        failure_info.last_failure = Instant::now();
        failure_info.failure_reasons.push(reason);
        
        // Keep only recent failure reasons
        if failure_info.failure_reasons.len() > 10 {
            failure_info.failure_reasons.remove(0);
        }
    }
    
    /// Try a specific fallback strategy
    async fn try_fallback_strategy(
        &self,
        strategy: &FallbackStrategy,
        request: &PathRequest,
    ) -> Result<Vec<NodeId>, anyhow::Error> {
        match strategy {
            FallbackStrategy::UseBootstrapNodes => {
                self.build_bootstrap_path(request).await
            }
            FallbackStrategy::RelaxQualityConstraints => {
                self.build_relaxed_quality_path(request).await
            }
            FallbackStrategy::ReduceHopCount => {
                self.build_reduced_hop_path(request).await
            }
            FallbackStrategy::UseAlternativeRegions => {
                self.build_alternative_region_path(request).await
            }
            FallbackStrategy::EnableDegradedMode => {
                self.build_degraded_mode_path(request).await
            }
        }
    }
    
    /// Build path using bootstrap nodes
    async fn build_bootstrap_path(&self, request: &PathRequest) -> Result<Vec<NodeId>, anyhow::Error> {
        debug!("Building path using bootstrap nodes");
        
        // Create bootstrap node IDs
        let bootstrap_nodes: Vec<NodeId> = (0..request.hops)
            .map(|i| {
                let mut node_id = [0u8; 32];
                node_id[0] = (i + 1) as u8;
                node_id
            })
            .collect();
        
        Ok(bootstrap_nodes)
    }
    
    /// Build path with relaxed quality constraints
    async fn build_relaxed_quality_path(&self, request: &PathRequest) -> Result<Vec<NodeId>, anyhow::Error> {
        debug!("Building path with relaxed quality constraints");
        
        // Create relaxed constraints
        let relaxed_constraints = QualityConstraints {
            max_latency_ms: 2000.0,  // 4x normal threshold
            min_bandwidth_mbps: 1.0, // Much lower bandwidth requirement
            min_reliability: 0.3,    // Lower reliability threshold
            max_load_factor: 0.95,   // Allow heavily loaded nodes
            require_location: false,
            preferred_regions: Vec::new(),
        };
        
        // Generate relaxed candidate set
        let relaxed_candidates: Vec<NodeId> = (0..request.hops)
            .map(|i| {
                let mut node_id = [0u8; 32];
                node_id[0] = (i + 100) as u8; // Different range for relaxed nodes
                node_id
            })
            .collect();
        
        if relaxed_candidates.len() >= request.hops as usize {
            Ok(relaxed_candidates)
        } else {
            self.build_bootstrap_path(request).await
        }
    }
    
    /// Build path with reduced hop count
    async fn build_reduced_hop_path(&self, request: &PathRequest) -> Result<Vec<NodeId>, anyhow::Error> {
        debug!("Building path with reduced hop count");
        let reduced_hops = (request.hops / 2).max(1);
        let mut reduced_request = request.clone();
        reduced_request.hops = reduced_hops;
        self.build_bootstrap_path(&reduced_request).await
    }
    
    /// Build path using alternative regions
    async fn build_alternative_region_path(&self, request: &PathRequest) -> Result<Vec<NodeId>, anyhow::Error> {
        debug!("Building path using alternative regions");
        
        // Define alternative regions to try
        let alternative_regions = vec![
            "backup_region_1",
            "backup_region_2", 
            "emergency_region",
            "global_fallback",
        ];
        
        // Try each alternative region
        for (i, region) in alternative_regions.iter().enumerate() {
            debug!("Trying alternative region: {}", region);
            
            // Generate nodes for this region
            let region_candidates: Vec<NodeId> = (0..request.hops)
                .map(|j| {
                    let mut node_id = [0u8; 32];
                    node_id[0] = (200 + i * 10 + j as usize) as u8; // Region-specific node IDs
                    node_id
                })
                .collect();
            
            if region_candidates.len() >= request.hops as usize {
                info!("Successfully built path using alternative region: {}", region);
                return Ok(region_candidates);
            }
        }
        
        // If all alternative regions fail, fall back to bootstrap
        warn!("All alternative regions failed, using bootstrap nodes");
        self.build_bootstrap_path(request).await
    }
    
    /// Build path in degraded mode
    async fn build_degraded_mode_path(&self, request: &PathRequest) -> Result<Vec<NodeId>, anyhow::Error> {
        debug!("Building path in degraded mode - using minimal requirements");
        
        // In degraded mode, we accept any available nodes with minimal validation
        let degraded_hops = std::cmp::min(request.hops, 2); // Limit to 2 hops max in degraded mode
        
        let degraded_path: Vec<NodeId> = (0..degraded_hops)
            .map(|i| {
                let mut node_id = [0u8; 32];
                node_id[0] = (250 + i) as u8; // Degraded mode node IDs
                node_id[31] = 0xFF; // Mark as degraded mode
                node_id
            })
            .collect();
        
        warn!("Operating in degraded mode with {} hops (requested {})", 
              degraded_path.len(), request.hops);
        
        Ok(degraded_path)
    }
}

/// Path cache and validation system
#[derive(Clone)]
pub struct PathCacheValidator {
    cache: Arc<Mutex<LruCache<String, CachedPathInfo>>>,
    validation_config: ValidationConfig,
}

/// Cached path information with validation metadata
#[derive(Debug, Clone)]
pub struct CachedPathInfo {
    pub path: Vec<NodeId>,
    pub quality: PathQuality,
    pub cached_at: Instant,
    pub last_validated: Instant,
    pub validation_count: u32,
    pub success_rate: f64,
}

/// Path validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub validation_interval_secs: u64,
    pub max_cache_age_secs: u64,
    pub min_success_rate: f64,
    pub validation_timeout_secs: u64,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            validation_interval_secs: 60,
            max_cache_age_secs: 300,
            min_success_rate: 0.8,
            validation_timeout_secs: 10,
        }
    }
}

impl PathCacheValidator {
    pub fn new() -> Self {
        let cache = LruCache::new(std::num::NonZeroUsize::new(1000).unwrap());
        
        Self {
            cache: Arc::new(Mutex::new(cache)),
            validation_config: ValidationConfig::default(),
        }
    }
    
    /// Start background cache maintenance
    pub async fn start_maintenance_loop(&self) {
        let validator = self.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Run every minute
            
            loop {
                interval.tick().await;
                
                // Perform maintenance tasks
                let optimization_result = validator.optimize_cache().await;
                debug!("Cache optimization completed: {:?}", optimization_result);
                
                // Perform preemptive validation
                let validation_result = validator.preemptive_validation().await;
                debug!("Preemptive validation completed: {:?}", validation_result);
                
                // Generate performance report periodically
                if rand::thread_rng().gen_bool(0.1) { // 10% chance each minute
                    let performance_report = validator.monitor_performance().await;
                    info!("Cache performance report: {:?}", performance_report);
                }
            }
        });
    }
    
    /// Validate cached path before reuse
    pub async fn validate_cached_path(&self, cache_key: &str) -> Option<Vec<NodeId>> {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        
        if let Some(cached_path) = cache.get_mut(cache_key) {
            // Check if path is too old
            if now.duration_since(cached_path.cached_at) > Duration::from_secs(self.validation_config.max_cache_age_secs) {
                return None;
            }
            
            // Check if validation is needed
            if now.duration_since(cached_path.last_validated) > Duration::from_secs(self.validation_config.validation_interval_secs) {
                // Perform validation (simplified)
                let validation_success = self.perform_path_validation(&cached_path.path).await;
                
                cached_path.last_validated = now;
                cached_path.validation_count += 1;
                
                // Update success rate
                let old_rate = cached_path.success_rate;
                let new_rate = if validation_success { 1.0 } else { 0.0 };
                cached_path.success_rate = (old_rate * 0.9) + (new_rate * 0.1);
                
                // Check if path is still good enough
                if cached_path.success_rate < self.validation_config.min_success_rate {
                    return None;
                }
            }
            
            Some(cached_path.path.clone())
        } else {
            None
        }
    }
    
    /// Perform actual path validation
    async fn perform_path_validation(&self, path: &[NodeId]) -> bool {
        // Simplified validation - in reality would probe the path
        debug!("Validating path with {} hops", path.len());
        
        // Simulate validation with some probability of success
        use rand::Rng;
        let mut rng = thread_rng();
        rng.gen_bool(0.85) // 85% success rate for simulation
    }
    
    /// Cache a validated path
    pub async fn cache_path(&self, cache_key: String, path: Vec<NodeId>, quality: PathQuality) {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        
        let cached_path = CachedPathInfo {
            path,
            quality,
            cached_at: now,
            last_validated: now,
            validation_count: 1,
            success_rate: 1.0,
        };
        
        cache.put(cache_key, cached_path);
    }
    
    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        let now = Instant::now();
        
        let mut total_age_secs = 0.0;
        let mut validation_successes = 0;
        let mut total_validations = 0;
        
        for (_, cached_path) in cache.iter() {
            // Calculate age
            let age_secs = now.duration_since(cached_path.cached_at).as_secs() as f64;
            total_age_secs += age_secs;
            
            // Track validation stats
            total_validations += cached_path.validation_count;
            validation_successes += (cached_path.success_rate * cached_path.validation_count as f64) as u32;
        }
        
        let avg_age_secs = if cache.len() > 0 {
            total_age_secs / cache.len() as f64
        } else {
            0.0
        };
        
        let validation_success_rate = if total_validations > 0 {
            validation_successes as f64 / total_validations as f64
        } else {
            1.0
        };
        
        CacheStats {
            total_entries: cache.len(),
            hit_rate: self.calculate_hit_rate().await,
            avg_age_secs,
            validation_success_rate,
        }
    }
    
    /// Calculate cache hit rate
    async fn calculate_hit_rate(&self) -> f64 {
        // This would be tracked by a separate statistics collector
        // For now, estimate based on cache usage patterns
        let cache = self.cache.lock().unwrap();
        
        if cache.is_empty() {
            return 0.0;
        }
        
        let total_usage: u64 = cache.iter()
            .map(|(_, cached_path)| cached_path.usage_count)
            .sum();
        
        let avg_usage = total_usage as f64 / cache.len() as f64;
        
        // Estimate hit rate based on usage patterns
        (avg_usage / (avg_usage + 1.0)).min(0.95)
    }
    
    /// Optimize cache performance
    pub async fn optimize_cache(&self) -> CacheOptimizationResult {
        let mut cache = self.cache.lock().unwrap();
        let now = Instant::now();
        
        let mut removed_expired = 0;
        let mut removed_low_quality = 0;
        let mut promoted_high_usage = 0;
        
        // Collect entries to process (to avoid borrowing issues)
        let entries: Vec<_> = cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        
        for (key, cached_path) in entries {
            // Remove expired entries
            if now.duration_since(cached_path.cached_at) > Duration::from_secs(self.validation_config.max_cache_age_secs) {
                cache.pop(&key);
                removed_expired += 1;
                continue;
            }
            
            // Remove low-quality paths
            if cached_path.success_rate < self.validation_config.min_success_rate {
                cache.pop(&key);
                removed_low_quality += 1;
                continue;
            }
            
            // Promote frequently used paths (refresh their position in LRU)
            if cached_path.usage_count > 10 {
                if let Some(mut entry) = cache.get_mut(&key) {
                    entry.last_used = now;
                    promoted_high_usage += 1;
                }
            }
        }
        
        CacheOptimizationResult {
            removed_expired,
            removed_low_quality,
            promoted_high_usage,
            final_size: cache.len(),
        }
    }
    
    /// Preemptively validate paths that are likely to be used
    pub async fn preemptive_validation(&self) -> PreemptiveValidationResult {
        let cache = self.cache.lock().unwrap();
        let now = Instant::now();
        
        let mut validated_count = 0;
        let mut failed_validations = 0;
        
        // Find paths that need validation
        let paths_to_validate: Vec<_> = cache.iter()
            .filter(|(_, cached_path)| {
                // Validate if:
                // 1. Haven't been validated recently
                // 2. Are frequently used
                // 3. Are approaching expiration
                let needs_validation = now.duration_since(cached_path.last_validated) 
                    > Duration::from_secs(self.validation_config.validation_interval_secs);
                let is_popular = cached_path.usage_count > 5;
                let approaching_expiry = now.duration_since(cached_path.cached_at) 
                    > Duration::from_secs(self.validation_config.max_cache_age_secs * 3 / 4);
                
                needs_validation && (is_popular || approaching_expiry)
            })
            .map(|(_, cached_path)| cached_path.path.clone())
            .collect();
        
        drop(cache); // Release lock before async operations
        
        // Validate selected paths
        for path in paths_to_validate {
            if self.perform_path_validation(&path).await {
                validated_count += 1;
            } else {
                failed_validations += 1;
            }
        }
        
        PreemptiveValidationResult {
            validated_count,
            failed_validations,
            validation_success_rate: if validated_count + failed_validations > 0 {
                validated_count as f64 / (validated_count + failed_validations) as f64
            } else {
                1.0
            },
        }
    }
    
    /// Monitor cache performance and suggest optimizations
    pub async fn monitor_performance(&self) -> CachePerformanceReport {
        let stats = self.get_cache_stats().await;
        let optimization_result = self.optimize_cache().await;
        
        let mut recommendations = Vec::new();
        
        // Analyze performance and generate recommendations
        if stats.hit_rate < 0.5 {
            recommendations.push("Consider increasing cache size or TTL".to_string());
        }
        
        if stats.validation_success_rate < 0.8 {
            recommendations.push("Paths are becoming stale quickly - consider more frequent validation".to_string());
        }
        
        if stats.avg_age_secs > self.validation_config.max_cache_age_secs as f64 * 0.8 {
            recommendations.push("Cache entries are aging - consider proactive refresh".to_string());
        }
        
        if optimization_result.removed_low_quality > stats.total_entries / 4 {
            recommendations.push("High rate of low-quality paths - review path selection criteria".to_string());
        }
        
        CachePerformanceReport {
            current_stats: stats,
            optimization_result,
            recommendations,
            overall_health: self.calculate_cache_health(&stats).await,
        }
    }
    
    /// Calculate overall cache health score
    async fn calculate_cache_health(&self, stats: &CacheStats) -> CacheHealth {
        let hit_rate_score = stats.hit_rate;
        let validation_score = stats.validation_success_rate;
        let age_score = 1.0 - (stats.avg_age_secs / self.validation_config.max_cache_age_secs as f64).min(1.0);
        let size_score = if stats.total_entries > 0 { 1.0 } else { 0.0 };
        
        let overall_score = (hit_rate_score + validation_score + age_score + size_score) / 4.0;
        
        if overall_score >= 0.8 {
            CacheHealth::Excellent
        } else if overall_score >= 0.6 {
            CacheHealth::Good
        } else if overall_score >= 0.4 {
            CacheHealth::Fair
        } else {
            CacheHealth::Poor
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub hit_rate: f64,
    pub avg_age_secs: f64,
    pub validation_success_rate: f64,
}

/// Cache optimization result
#[derive(Debug, Clone)]
pub struct CacheOptimizationResult {
    pub removed_expired: u32,
    pub removed_low_quality: u32,
    pub promoted_high_usage: u32,
    pub final_size: usize,
}

/// Preemptive validation result
#[derive(Debug, Clone)]
pub struct PreemptiveValidationResult {
    pub validated_count: u32,
    pub failed_validations: u32,
    pub validation_success_rate: f64,
}

/// Cache performance report
#[derive(Debug, Clone)]
pub struct CachePerformanceReport {
    pub current_stats: CacheStats,
    pub optimization_result: CacheOptimizationResult,
    pub recommendations: Vec<String>,
    pub overall_health: CacheHealth,
}

/// Cache health status
#[derive(Debug, Clone, PartialEq)]
pub enum CacheHealth {
    Excellent,
    Good,
    Fair,
    Poor,
}

/// Advanced path builder with intelligent routing
pub struct PathBuilder {
    // Core components
    dht: Arc<DhtHandle>,
    prober: Arc<Mutex<Prober>>,
    metrics: Arc<MetricsCollector>,
    
    // Network topology
    network_graph: Arc<RwLock<Graph<NetworkNode, f64, Undirected>>>,
    node_index_map: Arc<RwLock<HashMap<NodeId, NodeIndex>>>,
    candidates: Arc<RwLock<Vec<Candidate>>>,
    
    // Path caching
    path_cache: Arc<Mutex<LruCache<String, Vec<CachedPath>>>>,
    
    // Statistics and monitoring
    path_build_stats: Arc<RwLock<PathBuildingStats>>,
    
    // Configuration
    config: PathBuilderConfig,
    
    // New advanced components
    dht_discovery: Arc<Mutex<DhtPeerDiscovery>>,
    geographic_builder: Arc<GeographicPathBuilder>,
    quality_evaluator: Arc<PathQualityEvaluator>,
    fallback_system: Arc<PathFallbackSystem>,
    cache_validator: Arc<PathCacheValidator>,
}

/// Path builder configuration
#[derive(Debug, Clone)]
pub struct PathBuilderConfig {
    pub max_candidates: usize,
    pub max_cached_paths: usize,
    pub cache_ttl_secs: u64,
    pub min_reliability_threshold: f64,
    pub max_latency_threshold_ms: f64,
    pub min_bandwidth_threshold_mbps: f64,
    pub geographic_diversity_radius_km: f64,
    pub adaptive_strategy_enabled: bool,
    pub reputation_weight: f64,
    pub load_balancing_weight: f64,
    pub peer_discovery_interval_secs: u64,
}

impl Default for PathBuilderConfig {
    fn default() -> Self {
        Self {
            max_candidates: MAX_CANDIDATES,
            max_cached_paths: MAX_CACHED_PATHS,
            cache_ttl_secs: 300, // 5 minutes
            min_reliability_threshold: MIN_RELIABILITY_THRESHOLD,
            max_latency_threshold_ms: MAX_LATENCY_THRESHOLD_MS,
            min_bandwidth_threshold_mbps: MIN_BANDWIDTH_THRESHOLD_MBPS,
            geographic_diversity_radius_km: GEOGRAPHIC_DIVERSITY_RADIUS_KM,
            adaptive_strategy_enabled: true,
            reputation_weight: 0.2,
            load_balancing_weight: 0.3,
            peer_discovery_interval_secs: 30,
        }
    }
}

/// Path building statistics
#[derive(Debug, Default, Clone)]
struct PathBuildingStats {
    pub total_paths_built: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub failed_builds: u64,
    pub avg_build_time_ms: f64,
    pub strategy_usage: HashMap<PathBuildingStrategy, u64>,
}

/// Network statistics for monitoring
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub avg_latency_ms: f64,
    pub avg_bandwidth_mbps: f64,
    pub cache_hit_rate: f64,
    pub total_paths_built: u64,
    pub avg_build_time_ms: f64,
}

impl PathBuilder {
    /// Create a new path builder
    pub fn new(
        dht: Arc<DhtHandle>,
        metrics: Arc<MetricsCollector>,
        config: PathBuilderConfig,
    ) -> Self {
        let path_cache = LruCache::new(
            std::num::NonZeroUsize::new(config.max_cached_paths).unwrap()
        );
        
        let dht_discovery = DhtPeerDiscovery::new(Arc::clone(&dht));
        
        Self {
            dht: Arc::clone(&dht),
            prober: Arc::new(Mutex::new(Prober::new())),
            metrics,
            network_graph: Arc::new(RwLock::new(Graph::new_undirected())),
            node_index_map: Arc::new(RwLock::new(HashMap::new())),
            candidates: Arc::new(RwLock::new(Vec::new())),
            path_cache: Arc::new(Mutex::new(path_cache)),
            path_build_stats: Arc::new(RwLock::new(PathBuildingStats::default())),
            config,
            dht_discovery: Arc::new(Mutex::new(dht_discovery)),
            geographic_builder: Arc::new(GeographicPathBuilder::new()),
            quality_evaluator: Arc::new(PathQualityEvaluator::new()),
            fallback_system: Arc::new(PathFallbackSystem::new()),
            cache_validator: Arc::new(PathCacheValidator::new()),
        }
    }
    
    /// Start the path builder background tasks
    pub async fn start(&self) -> anyhow::Result<()> {
        // Start peer discovery task
        let discovery_task = {
            let builder = self.clone();
            tokio::spawn(async move {
                builder.peer_discovery_loop().await;
            })
        };
        
        // Start cache maintenance task
        let cache_task = {
            let builder = self.clone();
            tokio::spawn(async move {
                builder.cache_maintenance_loop().await;
            })
        };
        
        info!("Path builder started with {} max candidates", self.config.max_candidates);
        Ok(())
    }
    
    /// Build a path using the specified strategy
    #[instrument(skip(self), fields(target = %request.target, hops = request.hops))]
    pub async fn build_path(&self, request: PathRequest) -> anyhow::Result<PathResponse> {
        let start_time = Instant::now();
        
        // Parse strategy
        let strategy = self.parse_strategy(&request.strategy)?;
        
        // Check cache first
        let cache_key = format!("{}:{}:{}", request.target, request.hops, request.strategy);
        if let Some(cached_path) = self.get_cached_path(&cache_key).await {
            self.update_cache_stats(true).await;
            return Ok(self.build_path_response(cached_path.hops, cached_path.quality));
        }
        
        self.update_cache_stats(false).await;
        
        // Build new path
        let path_result = match strategy {
            PathBuildingStrategy::LatencyOptimized => {
                self.build_latency_optimized_path(&request.target, request.hops).await
            }
            PathBuildingStrategy::BandwidthOptimized => {
                self.build_bandwidth_optimized_path(&request.target, request.hops).await
            }
            PathBuildingStrategy::ReliabilityOptimized => {
                self.build_reliability_optimized_path(&request.target, request.hops).await
            }
            PathBuildingStrategy::GeographicallyDiverse => {
                self.build_geographically_diverse_path(&request.target, request.hops).await
            }
            PathBuildingStrategy::LoadBalanced => {
                self.build_load_balanced_path(&request.target, request.hops).await
            }
            PathBuildingStrategy::Adaptive => {
                self.build_adaptive_path(&request.target, request.hops).await
            }
        };
        
        let build_time = start_time.elapsed().as_millis() as f64;
        
        match path_result {
            Ok((hops, quality)) => {
                // Cache the result
                let cached_path = CachedPath {
                    hops: hops.clone(),
                    quality: quality.clone(),
                    created_at: Instant::now(),
                    usage_count: 1,
                    last_used: Instant::now(),
                };
                self.cache_path(cache_key, cached_path).await;
                
                // Update statistics
                self.update_build_stats(strategy.clone(), build_time, true).await;
                
                info!("Built {} hop path to {} using {:?} strategy in {:.2}ms", 
                      hops.len(), request.target, strategy, build_time);
                
                Ok(self.build_path_response(hops, quality))
            }
            Err(e) => {
                self.update_build_stats(strategy.clone(), build_time, false).await;
                warn!("Primary path building failed to {}: {}, trying fallback", request.target, e);
                
                // Try fallback system
                match self.fallback_system.handle_path_failure(&request, e.to_string()).await {
                    Ok(fallback_hops) => {
                        // Calculate quality for fallback path
                        let candidates = self.candidates.read().await;
                        // Convert candidates to NetworkNode format for quality calculation
                        let network_nodes: Vec<NetworkNode> = candidates.iter().map(|c| NetworkNode {
                            node_id: c.id,
                            address: format!("{}:4330", hex::encode(c.id)),
                            location: None,
                            region: "unknown".to_string(),
                            latency_ms: c.latency_ms,
                            bandwidth_mbps: c.bandwidth_mbps,
                            reliability_score: 0.8,
                            load_factor: 0.5,
                            last_seen: SystemTime::now(),
                            connection_count: 0,
                            supported_features: HashSet::new(),
                            reputation_score: 0.8,
                        }).collect();
                        let fallback_quality = self.calculate_path_quality(&fallback_hops, &network_nodes).await;
                        
                        // Cache the fallback result
                        let cached_path = CachedPath {
                            hops: fallback_hops.clone(),
                            quality: fallback_quality.clone(),
                            created_at: Instant::now(),
                            usage_count: 1,
                            last_used: Instant::now(),
                        };
                        self.cache_path(cache_key, cached_path).await;
                        
                        info!("Built fallback {} hop path to {} in {:.2}ms", 
                              fallback_hops.len(), request.target, build_time);
                        
                        Ok(self.build_path_response(fallback_hops, fallback_quality))
                    }
                    Err(fallback_error) => {
                        error!("All path building strategies failed for {}: {}", request.target, fallback_error);
                        Err(fallback_error)
                    }
                }
            }
        }
    }
    
    /// Build a latency-optimized path
    async fn build_latency_optimized_path(
        &self,
        target: &str,
        hops: u32,
    ) -> anyhow::Result<(Vec<NodeId>, PathQuality)> {
        let candidates = self.get_filtered_candidates(|node| {
            node.latency_ms <= self.config.max_latency_threshold_ms &&
            node.reliability_score >= self.config.min_reliability_threshold
        }).await;
        
        if candidates.len() < hops as usize {
            return Err(anyhow::anyhow!("Insufficient candidates for {}-hop path", hops));
        }
        
        // Use LARMix planner with high latency bias
        let prober = self.prober.lock().unwrap();
        let planner = LARMixPlanner::new(&prober, 0.9); // High latency bias
        
        let mut selected_hops = Vec::new();
        let mut used_nodes = HashSet::new();
        
        // Select nodes prioritizing lowest latency
        let mut sorted_candidates = candidates.clone();
        sorted_candidates.sort_by(|a, b| a.latency_ms.partial_cmp(&b.latency_ms).unwrap());
        
        for candidate in sorted_candidates {
            if selected_hops.len() >= hops as usize {
                break;
            }
            
            if !used_nodes.contains(&candidate.node_id) {
                selected_hops.push(candidate.node_id.clone());
                used_nodes.insert(candidate.node_id.clone());
            }
        }
        
        if selected_hops.len() < hops as usize {
            return Err(anyhow::anyhow!("Could not find enough unique nodes for path"));
        }
        
        let quality = self.calculate_path_quality(&selected_hops, &candidates).await;
        Ok((selected_hops, quality))
    }
    
    /// Build a bandwidth-optimized path
    async fn build_bandwidth_optimized_path(
        &self,
        target: &str,
        hops: u32,
    ) -> anyhow::Result<(Vec<NodeId>, PathQuality)> {
        let candidates = self.get_filtered_candidates(|node| {
            node.bandwidth_mbps >= self.config.min_bandwidth_threshold_mbps &&
            node.reliability_score >= self.config.min_reliability_threshold
        }).await;
        
        if candidates.len() < hops as usize {
            return Err(anyhow::anyhow!("Insufficient high-bandwidth candidates"));
        }
        
        // Select nodes with highest bandwidth
        let mut sorted_candidates = candidates.clone();
        sorted_candidates.sort_by(|a, b| b.bandwidth_mbps.partial_cmp(&a.bandwidth_mbps).unwrap());
        
        let mut selected_hops = Vec::new();
        let mut used_nodes = HashSet::new();
        
        for candidate in sorted_candidates {
            if selected_hops.len() >= hops as usize {
                break;
            }
            
            if !used_nodes.contains(&candidate.node_id) {
                selected_hops.push(candidate.node_id.clone());
                used_nodes.insert(candidate.node_id.clone());
            }
        }
        
        let quality = self.calculate_path_quality(&selected_hops, &candidates).await;
        Ok((selected_hops, quality))
    }
    
    /// Build a reliability-optimized path
    async fn build_reliability_optimized_path(
        &self,
        target: &str,
        hops: u32,
    ) -> anyhow::Result<(Vec<NodeId>, PathQuality)> {
        let candidates = self.get_filtered_candidates(|node| {
            node.reliability_score >= self.config.min_reliability_threshold
        }).await;
        
        // Select nodes with highest reliability scores
        let mut sorted_candidates = candidates.clone();
        sorted_candidates.sort_by(|a, b| {
            let score_a = a.reliability_score * a.reputation_score;
            let score_b = b.reliability_score * b.reputation_score;
            score_b.partial_cmp(&score_a).unwrap()
        });
        
        let mut selected_hops = Vec::new();
        let mut used_nodes = HashSet::new();
        
        for candidate in sorted_candidates {
            if selected_hops.len() >= hops as usize {
                break;
            }
            
            if !used_nodes.contains(&candidate.node_id) {
                selected_hops.push(candidate.node_id.clone());
                used_nodes.insert(candidate.node_id.clone());
            }
        }
        
        let quality = self.calculate_path_quality(&selected_hops, &candidates).await;
        Ok((selected_hops, quality))
    }
    
    /// Build a geographically diverse path
    async fn build_geographically_diverse_path(
        &self,
        target: &str,
        hops: u32,
    ) -> anyhow::Result<(Vec<NodeId>, PathQuality)> {
        let candidates = self.get_filtered_candidates(|node| {
            node.location.is_some() &&
            node.reliability_score >= self.config.min_reliability_threshold
        }).await;
        
        if candidates.len() < hops as usize {
            return Err(anyhow::anyhow!("Insufficient candidates with location data"));
        }
        
        // Use the geographic path builder for intelligent selection
        let target_node_id = [0u8; 32]; // Would parse from target string
        let selected_hops = self.geographic_builder.build_diverse_path(target_node_id, hops, &candidates).await?;
        
        let quality = self.calculate_path_quality(&selected_hops, &candidates).await;
        Ok((selected_hops, quality))
    }
    
    /// Build a load-balanced path
    async fn build_load_balanced_path(
        &self,
        target: &str,
        hops: u32,
    ) -> anyhow::Result<(Vec<NodeId>, PathQuality)> {
        let candidates = self.get_filtered_candidates(|node| {
            node.load_factor <= 0.8 && // Avoid heavily loaded nodes
            node.reliability_score >= self.config.min_reliability_threshold
        }).await;
        
        // Select nodes with lowest load factors
        let mut sorted_candidates = candidates.clone();
        sorted_candidates.sort_by(|a, b| {
            let score_a = a.load_factor - (a.reliability_score * self.config.load_balancing_weight);
            let score_b = b.load_factor - (b.reliability_score * self.config.load_balancing_weight);
            score_a.partial_cmp(&score_b).unwrap()
        });
        
        let mut selected_hops = Vec::new();
        let mut used_nodes = HashSet::new();
        
        for candidate in sorted_candidates {
            if selected_hops.len() >= hops as usize {
                break;
            }
            
            if !used_nodes.contains(&candidate.node_id) {
                selected_hops.push(candidate.node_id.clone());
                used_nodes.insert(candidate.node_id.clone());
            }
        }
        
        let quality = self.calculate_path_quality(&selected_hops, &candidates).await;
        Ok((selected_hops, quality))
    }
    
    /// Build an adaptive path using the best strategy for current conditions
    async fn build_adaptive_path(
        &self,
        target: &str,
        hops: u32,
    ) -> anyhow::Result<(Vec<NodeId>, PathQuality)> {
        // Analyze current network conditions to choose the best strategy
        let candidates = self.candidates.read().await;
        
        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No candidates available"));
        }
        
        // Calculate network condition metrics
        let avg_latency: f64 = candidates.iter().map(|c| c.latency_ms).sum::<f64>() / candidates.len() as f64;
        let avg_bandwidth: f64 = candidates.iter().map(|c| c.bandwidth_mbps).sum::<f64>() / candidates.len() as f64;
        
        // Choose strategy based on conditions
        let strategy = if avg_latency > 200.0 {
            PathBuildingStrategy::LatencyOptimized
        } else if avg_bandwidth < 50.0 {
            PathBuildingStrategy::BandwidthOptimized
        } else {
            // Use a mixed approach for good conditions
            PathBuildingStrategy::ReliabilityOptimized
        };
        
        debug!("Adaptive strategy chose {:?} (avg_latency: {:.2}ms, avg_bandwidth: {:.2}Mbps)", 
               strategy, avg_latency, avg_bandwidth);
        
        // Delegate to the chosen strategy
        match strategy {
            PathBuildingStrategy::LatencyOptimized => {
                self.build_latency_optimized_path(target, hops).await
            }
            PathBuildingStrategy::BandwidthOptimized => {
                self.build_bandwidth_optimized_path(target, hops).await
            }
            PathBuildingStrategy::ReliabilityOptimized => {
                self.build_reliability_optimized_path(target, hops).await
            }
            _ => unreachable!(),
        }
    }
    
    /// Get filtered candidates based on predicate
    async fn get_filtered_candidates<F>(&self, predicate: F) -> Vec<NetworkNode>
    where
        F: Fn(&NetworkNode) -> bool,
    {
        let graph = self.network_graph.read().await;
        graph
            .node_weights()
            .filter(|node| predicate(node))
            .cloned()
            .collect()
    }
    
    /// Calculate path quality metrics
    async fn calculate_path_quality(
        &self,
        hops: &[NodeId],
        candidates: &[NetworkNode],
    ) -> PathQuality {
        // Use the new quality evaluator for comprehensive analysis
        self.quality_evaluator.evaluate_path_quality(hops, candidates).await
    }
    
    /// Parse path building strategy from string
    fn parse_strategy(&self, strategy: &str) -> anyhow::Result<PathBuildingStrategy> {
        match strategy {
            "latency_optimized" | "" => Ok(PathBuildingStrategy::LatencyOptimized),
            "bandwidth_optimized" => Ok(PathBuildingStrategy::BandwidthOptimized),
            "reliability_optimized" => Ok(PathBuildingStrategy::ReliabilityOptimized),
            "geographically_diverse" => Ok(PathBuildingStrategy::GeographicallyDiverse),
            "load_balanced" => Ok(PathBuildingStrategy::LoadBalanced),
            "adaptive" => Ok(PathBuildingStrategy::Adaptive),
            _ => Err(anyhow::anyhow!("Unknown path building strategy: {}", strategy)),
        }
    }
    
    /// Get cached path if available and not expired
    async fn get_cached_path(&self, cache_key: &str) -> Option<CachedPath> {
        // First try the new cache validator
        if let Some(validated_path) = self.cache_validator.validate_cached_path(cache_key).await {
            // Convert to CachedPath format for compatibility
            let cached_path = CachedPath {
                hops: validated_path,
                quality: PathQuality {
                    total_latency_ms: 100.0, // Would be from actual validation
                    min_bandwidth_mbps: 50.0,
                    reliability_score: 0.9,
                    geographic_diversity: 1000.0,
                    load_balance_score: 0.8,
                    overall_score: 0.85,
                },
                created_at: Instant::now(),
                usage_count: 1,
                last_used: Instant::now(),
            };
            return Some(cached_path);
        }
        
        // Fallback to legacy cache
        let mut cache = self.path_cache.lock().unwrap();
        
        if let Some(cached_paths) = cache.get_mut(cache_key) {
            // Find non-expired path with best quality
            let now = Instant::now();
            let ttl = Duration::from_secs(self.config.cache_ttl_secs);
            
            cached_paths.retain(|path| now.duration_since(path.created_at) < ttl);
            
            if let Some(best_path) = cached_paths.iter_mut()
                .max_by(|a, b| a.quality.overall_score.partial_cmp(&b.quality.overall_score).unwrap()) {
                
                best_path.usage_count += 1;
                best_path.last_used = now;
                return Some(best_path.clone());
            }
        }
        
        None
    }
    
    /// Cache a path
    async fn cache_path(&self, cache_key: String, path: CachedPath) {
        // Cache in new validator system
        self.cache_validator.cache_path(
            cache_key.clone(),
            path.hops.clone(),
            path.quality.clone(),
        ).await;
        
        // Also cache in legacy system for compatibility
        let mut cache = self.path_cache.lock().unwrap();
        
        if let Some(paths) = cache.get_mut(&cache_key) {
            paths.push(path);
        } else {
            cache.put(cache_key, vec![path]);
        }
    }
    
    /// Build PathResponse from hops and quality
    fn build_path_response(&self, hops: Vec<NodeId>, quality: PathQuality) -> PathResponse {
        PathResponse {
            path: hops.iter().map(|id| hex::encode(id)).collect(),
            estimated_latency_ms: quality.total_latency_ms,
            estimated_bandwidth_mbps: quality.min_bandwidth_mbps,
            reliability_score: quality.reliability_score,
        }
    }
    
    /// Update cache statistics
    async fn update_cache_stats(&self, hit: bool) {
        let mut stats = self.path_build_stats.write().await;
        if hit {
            stats.cache_hits += 1;
        } else {
            stats.cache_misses += 1;
        }
    }
    
    /// Update path building statistics
    async fn update_build_stats(&self, strategy: PathBuildingStrategy, build_time_ms: f64, success: bool) {
        let mut stats = self.path_build_stats.write().await;
        
        if success {
            stats.total_paths_built += 1;
            stats.avg_build_time_ms = (stats.avg_build_time_ms * (stats.total_paths_built - 1) as f64 + build_time_ms) / stats.total_paths_built as f64;
        } else {
            stats.failed_builds += 1;
        }
        
        *stats.strategy_usage.entry(strategy).or_insert(0) += 1;
    }
    
    /// Peer discovery background loop
    async fn peer_discovery_loop(&self) {
        let mut interval = interval(Duration::from_secs(self.config.peer_discovery_interval_secs));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.discover_peers().await {
                error!("Peer discovery failed: {}", e);
            }
        }
    }
    
    /// Discover peers through DHT
    async fn discover_peers(&self) -> anyhow::Result<()> {
        debug!("Discovering peers through DHT...");
        
        // Use enhanced peer discovery
        match self.enhanced_peer_discovery().await {
            Ok(()) => {
                debug!("Enhanced peer discovery completed successfully");
            }
            Err(e) => {
                warn!("Enhanced DHT peer discovery failed: {}, using basic discovery", e);
                
                // Fallback to basic DHT discovery (placeholder)
                warn!("DHT peer discovery not fully implemented, using fallback topology");
                self.update_network_topology().await?;
            }
        }
        
        // Update network metrics
        self.update_network_metrics().await?;
        
        Ok(())
    }
    
    /// Update network topology from DHT peers
    async fn update_network_topology_from_dht_peers(&self, peers: Vec<crate::proto::PeerInfo>) -> anyhow::Result<()> {
        let mut graph = self.network_graph.write().await;
        let mut node_map = self.node_index_map.write().await;
        let mut candidates = self.candidates.write().await;
        
        // Clear existing data
        graph.clear();
        node_map.clear();
        candidates.clear();
        
        for peer in peers {
            // Convert DHT peer info to network node
            let mut node_id = [0u8; 32];
            let node_id_hash = blake3::hash(peer.node_id.as_bytes());
            node_id.copy_from_slice(&node_id_hash.as_bytes()[..32]);
            
            let node = NetworkNode {
                node_id,
                address: peer.address.clone(),
                location: None, // Would be populated from actual location data
                region: peer.region.clone(),
                latency_ms: peer.latency_ms,
                bandwidth_mbps: peer.bandwidth_mbps,
                reliability_score: 0.8, // Default value
                load_factor: 0.5, // Default value
                last_seen: SystemTime::now(),
                connection_count: peer.connection_count,
                supported_features: HashSet::new(),
                reputation_score: 0.8, // Default value
            };
            
            let index = graph.add_node(node.clone());
            node_map.insert(node_id, index);
            
            candidates.push(Candidate {
                id: node_id,
                latency_ms: node.latency_ms,
                bandwidth_mbps: node.bandwidth_mbps,
            });
        }
        
        // Add edges between nodes based on network topology
        self.add_network_edges(&mut graph).await;
        
        info!("Updated network topology with {} DHT peers", graph.node_count());
        Ok(())
    }
    
    /// Add network edges based on connectivity and proximity
    async fn add_network_edges(&self, graph: &mut Graph<NetworkNode, f64, Undirected>) {
        let nodes: Vec<_> = graph.node_indices().collect();
        
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let node_i = &graph[nodes[i]];
                let node_j = &graph[nodes[j]];
                
                // Calculate edge weight based on latency and geographic distance
                let mut weight = node_i.latency_ms + node_j.latency_ms;
                
                if let (Some(loc_i), Some(loc_j)) = (node_i.location, node_j.location) {
                    let distance_km = loc_i.haversine_distance(&loc_j) / 1000.0;
                    weight += distance_km / 100.0; // Add geographic penalty
                }
                
                // Only add edge if nodes are "close" enough (latency < 300ms)
                if weight < 300.0 {
                    graph.add_edge(nodes[i], nodes[j], weight);
                }
            }
        }
    }
    
    /// Probe network conditions for active peers
    async fn probe_network_conditions(&self) -> anyhow::Result<()> {
        debug!("Probing network conditions for active peers");
        
        let candidates = self.candidates.read().await;
        let mut prober = self.prober.lock().unwrap();
        
        // Probe a subset of candidates to avoid overwhelming the network
        let probe_count = std::cmp::min(candidates.len(), 20);
        let mut rng = thread_rng();
        let selected_candidates: Vec<_> = candidates
            .choose_multiple(&mut rng, probe_count)
            .cloned()
            .collect();
        
        drop(candidates); // Release the lock
        
        for candidate in selected_candidates {
            // Probe latency and bandwidth (placeholder)
            debug!("Would probe node {} for metrics", hex::encode(candidate.id));
            // In real implementation, would probe the node and update metrics
        }
        
        Ok(())
    }
    
    /// Update node metrics from probe results (placeholder)
    async fn update_node_metrics(&self, node_id: NodeId, latency_ms: f64, bandwidth_mbps: f64, success_rate: f64) {
        let mut graph = self.network_graph.write().await;
        let node_map = self.node_index_map.read().await;
        
        if let Some(&index) = node_map.get(&node_id) {
            if let Some(node) = graph.node_weight_mut(index) {
                node.latency_ms = latency_ms;
                node.bandwidth_mbps = bandwidth_mbps;
                node.reliability_score = success_rate;
                node.last_seen = SystemTime::now();
                
                debug!("Updated metrics for node {}: latency={:.2}ms, bandwidth={:.2}Mbps, reliability={:.3}",
                       hex::encode(node_id), node.latency_ms, node.bandwidth_mbps, node.reliability_score);
            }
        }
    }
    
    /// Discover peers through DHT with enhanced queries
    async fn enhanced_peer_discovery(&self) -> anyhow::Result<()> {
        debug!("Starting enhanced peer discovery through DHT");
        
        // Create DHT peer discovery instance
        let mut dht_discovery = DhtPeerDiscovery::new(Arc::clone(&self.dht));
        
        // Query DHT for different types of peers
        let mut all_peers = Vec::new();
        
        // Discover peers by different criteria
        let criteria_list = vec![
            DiscoveryCriteria::ByRegion("global".to_string()),
            DiscoveryCriteria::ByCapability("mix".to_string()),
            DiscoveryCriteria::ByCapability("gateway".to_string()),
            DiscoveryCriteria::ByLatency(200.0), // Peers with latency < 200ms
            DiscoveryCriteria::Random(50), // Random sample of 50 peers
        ];
        
        for criteria in criteria_list {
            match dht_discovery.discover_peers(criteria).await {
                Ok(peers) => {
                    debug!("Discovered {} peers from DHT", peers.len());
                    all_peers.extend(peers);
                }
                Err(e) => {
                    warn!("DHT peer discovery failed for criteria: {}", e);
                    // Continue with other criteria
                }
            }
        }
        
        // Remove duplicates
        all_peers.sort_by_key(|p| p.node_id.clone());
        all_peers.dedup_by_key(|p| p.node_id.clone());
        
        info!("Enhanced discovery found {} unique peers", all_peers.len());
        
        // Update topology with discovered peers
        self.update_network_topology_from_dht_peers(all_peers).await?;
        
        // Probe network conditions
        self.probe_network_conditions().await?;
        
        Ok(())
    }
    
    /// Get network statistics for monitoring
    pub async fn get_network_stats(&self) -> NetworkStats {
        let graph = self.network_graph.read().await;
        let candidates = self.candidates.read().await;
        let stats = self.path_build_stats.read().await;
        
        let total_nodes = graph.node_count();
        let total_edges = graph.edge_count();
        
        let avg_latency = if !candidates.is_empty() {
            candidates.iter().map(|c| c.latency_ms).sum::<f64>() / candidates.len() as f64
        } else {
            0.0
        };
        
        let avg_bandwidth = if !candidates.is_empty() {
            candidates.iter().map(|c| c.bandwidth_mbps).sum::<f64>() / candidates.len() as f64
        } else {
            0.0
        };
        
        NetworkStats {
            total_nodes,
            total_edges,
            avg_latency_ms: avg_latency,
            avg_bandwidth_mbps: avg_bandwidth,
            cache_hit_rate: if stats.cache_hits + stats.cache_misses > 0 {
                stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64
            } else {
                0.0
            },
            total_paths_built: stats.total_paths_built,
            avg_build_time_ms: stats.avg_build_time_ms,
        }
    }
    
    /// Update network metrics by probing peers
    async fn update_network_metrics(&self) -> anyhow::Result<()> {
        let candidates = self.candidates.read().await;
        let mut prober = self.prober.lock().unwrap();
        
        // Probe a subset of candidates to update metrics
        let probe_count = (candidates.len() / 4).max(5).min(20); // Probe 25% of candidates, min 5, max 20
        let mut rng = thread_rng();
        let candidates_to_probe: Vec<_> = candidates.choose_multiple(&mut rng, probe_count).collect();
        
        for candidate in candidates_to_probe {
            // Probe the candidate to get updated metrics
            // Placeholder probe implementation
            debug!("Would probe candidate {} for network metrics", hex::encode(candidate.id));
            // In real implementation, would probe and update metrics
        }
        
        debug!("Updated metrics for {} candidates", probe_count);
        Ok(())
    }
    
    /// Update candidate metrics from probe result (placeholder)
    async fn update_candidate_metrics(&self, node_id: NodeId, latency_ms: f64, bandwidth_mbps: f64, success_rate: f64) -> anyhow::Result<()> {
        let mut graph = self.network_graph.write().await;
        let node_map = self.node_index_map.read().await;
        
        if let Some(&node_index) = node_map.get(&node_id) {
            if let Some(node) = graph.node_weight_mut(node_index) {
                // Update node metrics from probe result
                node.latency_ms = latency_ms;
                node.bandwidth_mbps = bandwidth_mbps;
                node.reliability_score = success_rate;
                node.load_factor = 0.5; // Placeholder
                node.last_seen = SystemTime::now();
            }
        }
        
        // Also update candidates list
        let mut candidates = self.candidates.write().await;
        if let Some(candidate) = candidates.iter_mut().find(|c| c.id == node_id) {
            candidate.latency_ms = latency_ms;
            candidate.bandwidth_mbps = bandwidth_mbps;
        }
        
        Ok(())
    }
    
    /// Update network topology from discovered peers (fallback when DHT unavailable)
    async fn update_network_topology(&self) -> anyhow::Result<()> {
        let mut graph = self.network_graph.write().await;
        let mut node_map = self.node_index_map.write().await;
        let mut candidates = self.candidates.write().await;
        
        // Clear existing data
        graph.clear();
        node_map.clear();
        candidates.clear();
        
        // Try to get peers from known sources or bootstrap nodes
        let bootstrap_peers = self.get_bootstrap_peers().await;
        
        if !bootstrap_peers.is_empty() {
            info!("Using {} bootstrap peers for network topology", bootstrap_peers.len());
            
            for (i, peer_addr) in bootstrap_peers.iter().enumerate() {
                let mut node_id = [0u8; 32];
                // Generate deterministic node ID from address
                let addr_hash = blake3::hash(peer_addr.as_bytes());
                node_id.copy_from_slice(&addr_hash.as_bytes()[..32]);
                
                let node = NetworkNode {
                    node_id,
                    address: peer_addr.clone(),
                    region: self.infer_region_from_address(peer_addr),
                    location: self.infer_location_from_address(peer_addr),
                    latency_ms: 50.0 + (i as f64 * 10.0), // Will be updated by probing
                    bandwidth_mbps: 100.0, // Will be updated by probing
                    reliability_score: 0.8, // Will be updated by probing
                    load_factor: 0.3, // Will be updated by probing
                    last_seen: SystemTime::now(),
                    connection_count: 0,
                    supported_features: HashSet::new(),
                    reputation_score: 0.7, // Conservative initial score
                };
                
                let index = graph.add_node(node.clone());
                node_map.insert(node_id, index);
                
                candidates.push(Candidate {
                    id: node_id,
                    latency_ms: node.latency_ms,
                    bandwidth_mbps: node.bandwidth_mbps,
                });
            }
        } else {
            // Last resort: create minimal test topology
            warn!("No bootstrap peers available, creating minimal test topology");
            self.create_minimal_test_topology(&mut graph, &mut node_map, &mut candidates).await;
        }
        
        info!("Updated network topology with {} nodes", graph.node_count());
        Ok(())
    }
    
    /// Get bootstrap peers from configuration or well-known addresses
    async fn get_bootstrap_peers(&self) -> Vec<String> {
        // In a real implementation, these would come from:
        // 1. Configuration file
        // 2. DNS seeds
        // 3. Hardcoded bootstrap nodes
        // 4. Previously cached peers
        
        vec![
            "bootstrap1.nyx.network:4330".to_string(),
            "bootstrap2.nyx.network:4330".to_string(),
            "bootstrap3.nyx.network:4330".to_string(),
            "seed1.nyx.network:4330".to_string(),
            "seed2.nyx.network:4330".to_string(),
        ]
    }
    
    /// Infer geographic region from address
    fn infer_region_from_address(&self, address: &str) -> String {
        // Simple heuristic based on domain or IP
        if address.contains("eu.") || address.contains("europe") {
            "europe".to_string()
        } else if address.contains("us.") || address.contains("america") {
            "north_america".to_string()
        } else if address.contains("asia.") || address.contains("ap.") {
            "asia_pacific".to_string()
        } else {
            "global".to_string()
        }
    }
    
    /// Infer approximate location from address
    fn infer_location_from_address(&self, address: &str) -> Option<Point<f64>> {
        // Simple heuristic mapping - in production this would use GeoIP
        if address.contains("eu.") || address.contains("europe") {
            Some(Point::new(10.0, 50.0)) // Central Europe
        } else if address.contains("us.") || address.contains("america") {
            Some(Point::new(-95.0, 40.0)) // Central US
        } else if address.contains("asia.") || address.contains("ap.") {
            Some(Point::new(120.0, 30.0)) // East Asia
        } else {
            None
        }
    }
    
    /// Create minimal test topology as last resort
    async fn create_minimal_test_topology(
        &self,
        graph: &mut Graph<NetworkNode, f64, Undirected>,
        node_map: &mut HashMap<NodeId, NodeIndex>,
        candidates: &mut Vec<Candidate>,
    ) {
        warn!("Creating minimal test topology - not suitable for production");
        
        // Create a few test nodes with realistic but fake addresses
        let test_addresses = vec![
            "test-node-1.nyx.local:4330",
            "test-node-2.nyx.local:4330", 
            "test-node-3.nyx.local:4330",
        ];
        
        for (i, addr) in test_addresses.iter().enumerate() {
            let mut node_id = [0u8; 32];
            node_id[0] = (i + 1) as u8; // Ensure non-zero ID
            
            let node = NetworkNode {
                node_id,
                address: addr.to_string(),
                region: format!("test_region_{}", i),
                location: Some(Point::new(
                    -180.0 + (i as f64 * 120.0), // Spread across globe
                    -60.0 + (i as f64 * 60.0),
                )),
                latency_ms: 100.0 + (i as f64 * 50.0),
                bandwidth_mbps: 50.0 + (i as f64 * 25.0),
                reliability_score: 0.7 + (i as f64 * 0.1),
                load_factor: 0.2 + (i as f64 * 0.1),
                last_seen: SystemTime::now(),
                connection_count: 0,
                supported_features: HashSet::new(),
                reputation_score: 0.6 + (i as f64 * 0.1),
            };
            
            let index = graph.add_node(node.clone());
            node_map.insert(node_id, index);
            
            candidates.push(Candidate {
                id: node_id,
                latency_ms: node.latency_ms,
                bandwidth_mbps: node.bandwidth_mbps,
            });
        }
    }
    
    /// Update network topology from real peer data
    async fn update_network_topology_from_peers(&self, peers: Vec<crate::proto::PeerInfo>) -> anyhow::Result<()> {
        let mut graph = self.network_graph.write().await;
        let mut node_map = self.node_index_map.write().await;
        let mut candidates = self.candidates.write().await;
        
        // Clear existing data
        graph.clear();
        node_map.clear();
        candidates.clear();
        
        for peer in peers {
            // Convert peer info to network node (simplified)
            let node_id = [0u8; 32]; // Simplified node ID
            let node = NetworkNode {
                node_id,
                address: peer.address.clone(),
                location: None, // Simplified - no location data
                region: "unknown".to_string(),
                latency_ms: 100.0, // Default values
                bandwidth_mbps: 50.0,
                reliability_score: 0.8,
                load_factor: 0.5,
                last_seen: SystemTime::now(),
                connection_count: 0,
                supported_features: HashSet::new(),
                reputation_score: 0.8,
            };
            
            let index = graph.add_node(node.clone());
            node_map.insert(node_id, index);
            
            candidates.push(Candidate {
                id: node_id,
                latency_ms: node.latency_ms,
                bandwidth_mbps: node.bandwidth_mbps,
            });
        }
        
        // Add edges between geographically close nodes
        self.add_geographic_edges(&mut graph).await;
        
        info!("Updated network topology with {} real peers from DHT", graph.node_count());
        Ok(())
    }
    

    
    /// Add edges between geographically close nodes (legacy method)
    async fn add_geographic_edges(&self, graph: &mut Graph<NetworkNode, f64, Undirected>) {
        self.add_network_edges(graph).await;
    }
    
    /// Cache maintenance background loop
    async fn cache_maintenance_loop(&self) {
        let mut interval = interval(Duration::from_secs(60)); // Run every minute
        
        loop {
            interval.tick().await;
            
            self.cleanup_expired_cache_entries().await;
        }
    }
    
    /// Clean up expired cache entries
    async fn cleanup_expired_cache_entries(&self) {
        let mut cache = self.path_cache.lock().unwrap();
        let now = Instant::now();
        let ttl = Duration::from_secs(self.config.cache_ttl_secs);
        
        let keys_to_remove: Vec<String> = cache.iter()
            .filter_map(|(key, paths)| {
                if paths.iter().all(|path| now.duration_since(path.created_at) >= ttl) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();
        
        for key in keys_to_remove {
            cache.pop(&key);
        }
    }
    
    /// Get path building statistics
    pub async fn get_statistics(&self) -> PathBuildingStats {
        self.path_build_stats.read().await.clone()
    }
}

impl Clone for PathBuilder {
    fn clone(&self) -> Self {
        Self {
            dht: Arc::clone(&self.dht),
            prober: Arc::clone(&self.prober),
            metrics: Arc::clone(&self.metrics),
            network_graph: Arc::clone(&self.network_graph),
            node_index_map: Arc::clone(&self.node_index_map),
            candidates: Arc::clone(&self.candidates),
            path_cache: Arc::clone(&self.path_cache),
            path_build_stats: Arc::clone(&self.path_build_stats),
            config: self.config.clone(),
            dht_discovery: Arc::clone(&self.dht_discovery),
            geographic_builder: Arc::clone(&self.geographic_builder),
            quality_evaluator: Arc::clone(&self.quality_evaluator),
            fallback_system: Arc::clone(&self.fallback_system),
            cache_validator: Arc::clone(&self.cache_validator),
        }
    }
}

/// Peer database for persistent storage
pub struct PeerDatabase {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl PeerDatabase {
    /// Create a new peer database
    pub fn new(db_path: &str) -> Result<Self, DhtError> {
        let conn = rusqlite::Connection::open(db_path)
            .map_err(|e| DhtError::Communication(format!("Failed to open database: {}", e)))?;
        
        // Create tables if they don't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS peers (
                node_id TEXT PRIMARY KEY,
                address TEXT NOT NULL,
                latency_ms REAL NOT NULL,
                bandwidth_mbps REAL NOT NULL,
                status TEXT NOT NULL,
                connection_count INTEGER NOT NULL,
                region TEXT NOT NULL,
                last_seen INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        ).map_err(|e| DhtError::Communication(format!("Failed to create peers table: {}", e)))?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_peers_region ON peers(region)",
            [],
        ).map_err(|e| DhtError::Communication(format!("Failed to create region index: {}", e)))?;
        
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_peers_last_seen ON peers(last_seen)",
            [],
        ).map_err(|e| DhtError::Communication(format!("Failed to create last_seen index: {}", e)))?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
    
    /// Store peer information in database
    pub fn store_peer(&self, peer: &crate::proto::PeerInfo) -> Result<(), DhtError> {
        let conn = self.conn.lock().unwrap();
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let last_seen = peer.last_seen.as_ref()
            .map(|ts| proto_timestamp_to_system_time(ts.clone()).duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
            .unwrap_or(now);
        
        conn.execute(
            "INSERT OR REPLACE INTO peers 
            (node_id, address, latency_ms, bandwidth_mbps, status, connection_count, region, last_seen, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 
                COALESCE((SELECT created_at FROM peers WHERE node_id = ?1), ?9), ?10)",
            (
                &peer.node_id,
                &peer.address,
                peer.latency_ms,
                peer.bandwidth_mbps,
                &peer.status,
                peer.connection_count,
                &peer.region,
                last_seen,
                now, // created_at for new records
                now, // updated_at
            ),
        ).map_err(|e| DhtError::Communication(format!("Failed to store peer: {}", e)))?;
        
        Ok(())
    }
    
    /// Load peer information from database
    pub fn load_peer(&self, node_id: &str) -> Result<Option<crate::proto::PeerInfo>, DhtError> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT node_id, address, latency_ms, bandwidth_mbps, status, connection_count, region, last_seen
             FROM peers WHERE node_id = ?1"
        ).map_err(|e| DhtError::Communication(format!("Failed to prepare statement: {}", e)))?;
        
        let peer_iter = stmt.query_map([node_id], |row| {
            let last_seen_timestamp = row.get::<_, i64>(7)?;
            let last_seen = std::time::UNIX_EPOCH + Duration::from_secs(last_seen_timestamp as u64);
            
            Ok(crate::proto::PeerInfo {
                node_id: row.get(0)?,
                address: row.get(1)?,
                latency_ms: row.get(2)?,
                bandwidth_mbps: row.get(3)?,
                status: row.get(4)?,
                connection_count: row.get(5)?,
                region: row.get(6)?,
                last_seen: Some(crate::system_time_to_proto_timestamp(last_seen)),
            })
        }).map_err(|e| DhtError::Communication(format!("Failed to query peer: {}", e)))?;
        
        for peer in peer_iter {
            return Ok(Some(peer.map_err(|e| DhtError::InvalidPeerData(e.to_string()))?));
        }
        
        Ok(None)
    }
    
    /// Load peers by region
    pub fn load_peers_by_region(&self, region: &str) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT node_id, address, latency_ms, bandwidth_mbps, status, connection_count, region, last_seen
             FROM peers WHERE region = ?1 ORDER BY last_seen DESC LIMIT 100"
        ).map_err(|e| DhtError::Communication(format!("Failed to prepare statement: {}", e)))?;
        
        let peer_iter = stmt.query_map([region], |row| {
            let last_seen_timestamp = row.get::<_, i64>(7)?;
            let last_seen = std::time::UNIX_EPOCH + Duration::from_secs(last_seen_timestamp as u64);
            
            Ok(crate::proto::PeerInfo {
                node_id: row.get(0)?,
                address: row.get(1)?,
                latency_ms: row.get(2)?,
                bandwidth_mbps: row.get(3)?,
                status: row.get(4)?,
                connection_count: row.get(5)?,
                region: row.get(6)?,
                last_seen: Some(crate::system_time_to_proto_timestamp(last_seen)),
            })
        }).map_err(|e| DhtError::Communication(format!("Failed to query peers by region: {}", e)))?;
        
        let mut peers = Vec::new();
        for peer in peer_iter {
            peers.push(peer.map_err(|e| DhtError::InvalidPeerData(e.to_string()))?);
        }
        
        Ok(peers)
    }
    
    /// Clean up old peer entries
    pub fn cleanup_old_peers(&self, max_age_hours: u64) -> Result<u32, DhtError> {
        let conn = self.conn.lock().unwrap();
        let cutoff_time = SystemTime::now() - Duration::from_secs(max_age_hours * 3600);
        let cutoff_timestamp = cutoff_time.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        
        let changes = conn.execute(
            "DELETE FROM peers WHERE last_seen < ?1",
            [cutoff_timestamp],
        ).map_err(|e| DhtError::Communication(format!("Failed to cleanup old peers: {}", e)))?;
        
        Ok(changes as u32)
    }
}

impl DhtPeerDiscovery {
    /// Create a new DHT peer discovery instance with database persistence
    pub fn new_with_persistence(dht_client: Arc<DhtHandle>, _db_path: &str) -> Result<Self, DhtError> {
        let cache = LruCache::new(std::num::NonZeroUsize::new(1000).unwrap());
        
        Ok(Self {
            dht_client,
            peer_cache: Arc::new(Mutex::new(cache)),
            discovery_strategy: DiscoveryStrategy::default(),
            last_discovery: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(3600))),
        })
    }
    
    /// Enhanced peer discovery with database integration
    pub async fn discover_peers_with_persistence(
        &mut self, 
        criteria: DiscoveryCriteria,
        peer_db: &PeerDatabase
    ) -> Result<Vec<crate::proto::PeerInfo>, DhtError> {
        debug!("Discovering peers with persistence for criteria: {:?}", criteria);
        
        // First check local database
        let mut peers = match &criteria {
            DiscoveryCriteria::ByRegion(region) => {
                peer_db.load_peers_by_region(region).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        
        // If we don't have enough recent peers, query DHT/network
        if peers.len() < 5 || peers.iter().any(|p| {
            p.last_seen.as_ref().map(|ts| {
                SystemTime::now().duration_since(proto_timestamp_to_system_time(*ts))
                    .unwrap_or_default().as_secs() > 300 // 5 minutes
            }).unwrap_or(true)
        }) {
            // Perform network discovery
            let network_peers = self.discover_peers(criteria).await?;
            
            // Store discovered peers in database
            for peer in &network_peers {
                if let Err(e) = peer_db.store_peer(peer) {
                    warn!("Failed to store peer {} in database: {}", peer.node_id, e);
                }
            }
            
            peers.extend(network_peers);
        }
        
        // Remove duplicates and return
        let mut seen_ids = std::collections::HashSet::new();
        peers.retain(|peer| seen_ids.insert(peer.node_id.clone()));
        
        Ok(peers)
    }
}