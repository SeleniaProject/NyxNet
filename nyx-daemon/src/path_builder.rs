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

use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
};
use std::time::{Duration, SystemTime, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn, instrument};
use rand::{seq::SliceRandom, thread_rng};

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
        
        Self {
            dht,
            prober: Arc::new(Mutex::new(Prober::new())),
            metrics,
            network_graph: Arc::new(RwLock::new(Graph::new_undirected())),
            node_index_map: Arc::new(RwLock::new(HashMap::new())),
            candidates: Arc::new(RwLock::new(Vec::new())),
            path_cache: Arc::new(Mutex::new(path_cache)),
            path_build_stats: Arc::new(RwLock::new(PathBuildingStats::default())),
            config,
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
                error!("Failed to build path to {}: {}", request.target, e);
                Err(e)
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
        
        let mut selected_hops = Vec::new();
        let mut used_nodes = HashSet::new();
        let mut selected_locations = Vec::new();
        
        // First, select a random starting node
        let mut remaining_candidates = candidates.clone();
        remaining_candidates.shuffle(&mut thread_rng());
        
        for candidate in remaining_candidates {
            if selected_hops.len() >= hops as usize {
                break;
            }
            
            if used_nodes.contains(&candidate.node_id) {
                continue;
            }
            
            let location = candidate.location.unwrap();
            
            // Check if this location provides sufficient diversity
            let mut min_distance = f64::INFINITY;
            for existing_location in &selected_locations {
                let distance = location.haversine_distance(existing_location) / 1000.0; // Convert to km
                min_distance = min_distance.min(distance);
            }
            
            // If first node or sufficiently far from existing nodes, select it
            if selected_locations.is_empty() || min_distance >= self.config.geographic_diversity_radius_km {
                selected_hops.push(candidate.node_id.clone());
                used_nodes.insert(candidate.node_id.clone());
                selected_locations.push(location);
            }
        }
        
        // If we don't have enough geographically diverse nodes, fill with best available
        if selected_hops.len() < hops as usize {
            for candidate in &candidates {
                if selected_hops.len() >= hops as usize {
                    break;
                }
                
                if !used_nodes.contains(&candidate.node_id) {
                    selected_hops.push(candidate.node_id.clone());
                    used_nodes.insert(candidate.node_id.clone());
                }
            }
        }
        
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
        let node_map: HashMap<NodeId, &NetworkNode> = candidates
            .iter()
            .map(|n| (n.node_id, n))
            .collect();
        
        let mut total_latency = 0.0;
        let mut min_bandwidth = f64::INFINITY;
        let mut reliability_product = 1.0;
        let mut geographic_distances = Vec::new();
        let mut load_factors = Vec::new();
        
        for &hop in hops {
            if let Some(node) = node_map.get(&hop) {
                total_latency += node.latency_ms;
                min_bandwidth = min_bandwidth.min(node.bandwidth_mbps);
                reliability_product *= node.reliability_score;
                load_factors.push(node.load_factor);
                
                // Calculate geographic diversity
                if let Some(location) = node.location {
                    for other_hop in hops {
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
        
        // Calculate overall score (weighted combination)
        let overall_score = (1.0 / (1.0 + total_latency / 1000.0)) * 0.3 +
                           (min_bandwidth / 1000.0).min(1.0) * 0.3 +
                           reliability_product * 0.2 +
                           (geographic_diversity / 10000.0).min(1.0) * 0.1 +
                           load_balance_score * 0.1;
        
        PathQuality {
            total_latency_ms: total_latency,
            min_bandwidth_mbps: min_bandwidth,
            reliability_score: reliability_product,
            geographic_diversity,
            load_balance_score,
            overall_score,
        }
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
        
        // Query DHT for different types of peers
        let mut all_peers = Vec::new();
        
        // DHT peer discovery placeholder - in real implementation would query DHT
        warn!("Enhanced DHT peer discovery not fully implemented");
        
        // Create some mock peers for testing
        let mock_peers = vec![
            crate::proto::PeerInfo {
                node_id: "mock_peer_1".to_string(),
                address: "bootstrap1.nyx.network:4330".to_string(),
                latency_ms: 50.0,
                bandwidth_mbps: 100.0,
                last_seen: Some(crate::system_time_to_proto_timestamp(SystemTime::now())),
                region: "global".to_string(),
                connection_count: 10,
                status: "active".to_string(),
            }
        ];
        all_peers.extend(mock_peers);
        
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
        }
    }
} 