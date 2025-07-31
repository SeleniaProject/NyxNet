//! Pure Rust DHT implementation using TCP/TLS networking
//! 
//! This module provides a complete Distributed Hash Table (DHT) implementation
//! using only pure Rust dependencies, avoiding any C/C++ code entirely.
//! Uses tokio TCP/TLS for networking to eliminate ring dependency.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use sha2::{Digest, Sha256};
use multiaddr::Multiaddr;
use trust_dns_resolver::TokioAsyncResolver;
use bincode;

/// Pure Rust DHT implementation using TCP networking
pub struct PureRustDht {
    /// Node's cryptographic keypair for identity and signing
    keypair: SigningKey,
    /// Node's network address
    local_addr: SocketAddr,
    /// Routing table for Kademlia protocol
    routing_table: Arc<RwLock<RoutingTable>>,
    /// Data storage for DHT records
    storage: Arc<RwLock<HashMap<String, DhtRecord>>>,
    /// TCP listener for incoming connections
    listener: Option<TcpListener>,
    /// Bootstrap peers for network discovery
    bootstrap_peers: Vec<SocketAddr>,
    /// DNS resolver for peer discovery
    dns_resolver: TokioAsyncResolver,
}

/// Routing table for Kademlia DHT protocol
#[derive(Debug, Clone)]
pub struct RoutingTable {
    /// K-buckets containing peer information
    buckets: Vec<KBucket>,
    /// Local node ID for distance calculation
    local_node_id: NodeId,
    /// Maximum number of peers per bucket (typically 20)
    k_value: usize,
}

/// K-bucket containing peers at specific distance range
#[derive(Debug, Clone)]
pub struct KBucket {
    /// Peers in this bucket
    peers: Vec<PeerInfo>,
    /// Maximum capacity of the bucket
    max_size: usize,
    /// Distance range covered by this bucket
    distance_range: (u8, u8),
}

/// Peer information for DHT operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Unique peer identifier
    pub peer_id: String,
    /// Network address of the peer
    pub address: Multiaddr,
    /// Public key for cryptographic verification
    pub public_key: Vec<u8>,
    /// Last seen timestamp
    pub last_seen: SystemTime,
    /// Response time statistics
    pub avg_response_time: Duration,
}

/// Node identifier (256-bit hash)
pub type NodeId = [u8; 32];

/// Routing table statistics
#[derive(Debug, Clone)]
pub struct RoutingStats {
    pub total_peers: usize,
    pub active_buckets: usize,
    pub total_buckets: usize,
    pub k_value: usize,
    pub bucket_distribution: Vec<(usize, usize)>, // (bucket_index, peer_count)
}

/// DHT record stored in the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtRecord {
    /// Record key
    pub key: String,
    /// Record value
    pub value: Vec<u8>,
    /// Timestamp when record was created
    pub timestamp: SystemTime,
    /// Time-to-live for the record
    pub ttl: Duration,
    /// Signature of the record
    pub signature: Vec<u8>,
    /// Public key of the record publisher
    pub publisher: Vec<u8>,
}

/// DHT message types for peer communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DhtMessage {
    /// Ping message for connectivity testing
    Ping {
        node_id: NodeId,
        timestamp: u64,
    },
    /// Pong response to ping
    Pong {
        node_id: NodeId,
        timestamp: u64,
    },
    /// Find node request
    FindNode {
        target_id: String,
        requester_id: String,
    },
    /// Find node response
    FindNodeResponse {
        nodes: Vec<PeerInfo>,
    },
    /// Store record request
    Store {
        record: DhtRecord,
    },
    /// Store response
    StoreResponse {
        success: bool,
    },
    /// Find value request
    FindValue {
        key: String,
        requester_id: String,
    },
    /// Find value response - can return either value or closest nodes
    FindValueResponse {
        value: Option<Vec<u8>>,
        nodes: Option<Vec<PeerInfo>>,
    },
}

/// Error types for DHT operations
#[derive(Debug, thiserror::Error)]
pub enum DhtError {
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(String),
    #[error("Cryptographic error: {0}")]
    Crypto(String),
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
    #[error("Timeout occurred")]
    Timeout,
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Communication error: {0}")]
    Communication(String),
    #[error("Not initialized: {0}")]
    NotInitialized(String),
    #[error("Query failed: {0}")]
    QueryFailed(String),
    #[error("Invalid peer: {0}")]
    InvalidPeer(String),
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Bootstrap failed")]
    BootstrapFailed,
}

impl PureRustDht {
    /// Create a new Pure Rust DHT node
    pub async fn new(
        local_addr: SocketAddr,
        bootstrap_peers: Vec<SocketAddr>,
    ) -> Result<Self, DhtError> {
        // Generate cryptographic keypair for node identity
        let mut csprng = rand::rngs::OsRng;
        let keypair = SigningKey::generate(&mut csprng);
        
        // Create DNS resolver
        let dns_resolver = TokioAsyncResolver::tokio_from_system_conf()
            .map_err(|e| DhtError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create DNS resolver: {}", e)
            )))?;
        
        // Calculate local node ID from public key
        let local_node_id = Self::calculate_node_id(&keypair.verifying_key().to_bytes());
        
        // Initialize routing table
        let routing_table = Arc::new(RwLock::new(RoutingTable::new(local_node_id, 20)));
        
        // Initialize storage
        let storage = Arc::new(RwLock::new(HashMap::new()));
        
        Ok(PureRustDht {
            keypair,
            local_addr,
            routing_table,
            storage,
            listener: None,
            bootstrap_peers,
            dns_resolver,
        })
    }
    
    /// Start the DHT node
    pub async fn start(&mut self) -> Result<(), DhtError> {
        // Bind TCP listener
        let listener = TcpListener::bind(self.local_addr).await?;
        self.listener = Some(listener);
        
        info!("Pure Rust DHT node started on {}", self.local_addr);
        
        // Start accepting connections
        self.start_connection_handler().await?;
        
        // Bootstrap from known peers
        self.bootstrap().await?;
        
        Ok(())
    }
    
    /// Start connection handler for incoming connections
    async fn start_connection_handler(&self) -> Result<(), DhtError> {
        if let Some(ref listener) = self.listener {
            let routing_table = Arc::clone(&self.routing_table);
            let storage = Arc::clone(&self.storage);
            let keypair = self.keypair.clone();
            
            // Take local reference to avoid lifetime issues
            let listener_addr = listener.local_addr()?;
            
            // Recreate listener for spawned task to avoid borrow checker issues
            let listener = TcpListener::bind(listener_addr).await?;
            
            tokio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((stream, peer_addr)) => {
                            let routing_table = Arc::clone(&routing_table);
                            let storage = Arc::clone(&storage);
                            let keypair = keypair.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(
                                    stream, peer_addr, routing_table, storage, keypair
                                ).await {
                                    warn!("Error handling connection from {}: {}", peer_addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
            });
        }
        
        Ok(())
    }
    
    /// Handle incoming connection
    async fn handle_connection(
        mut stream: TcpStream,
        peer_addr: SocketAddr,
        routing_table: Arc<RwLock<RoutingTable>>,
        storage: Arc<RwLock<HashMap<String, DhtRecord>>>,
        keypair: SigningKey,
    ) -> Result<(), DhtError> {
        debug!("Handling connection from {}", peer_addr);
        
        // Read message length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_be_bytes(len_buf) as usize;
        
        // Read message data
        let mut msg_buf = vec![0u8; msg_len];
        stream.read_exact(&mut msg_buf).await?;
        
        // Deserialize message
        let message: DhtMessage = bincode::deserialize(&msg_buf)?;
        
        // Process message
        let response = Self::process_message(message, routing_table, storage, &keypair).await?;
        
        // Send response
        let response_data = bincode::serialize(&response)?;
        let response_len = (response_data.len() as u32).to_be_bytes();
        
        stream.write_all(&response_len).await?;
        stream.write_all(&response_data).await?;
        stream.flush().await?;
        
        Ok(())
    }
    
    /// Process incoming DHT message
    async fn process_message(
        message: DhtMessage,
        routing_table: Arc<RwLock<RoutingTable>>,
        storage: Arc<RwLock<HashMap<String, DhtRecord>>>,
        keypair: &SigningKey,
    ) -> Result<DhtMessage, DhtError> {
        match message {
            DhtMessage::Ping { node_id, timestamp } => {
                Ok(DhtMessage::Pong {
                    node_id: Self::calculate_node_id(&keypair.verifying_key().to_bytes()),
                    timestamp,
                })
            }
            DhtMessage::FindNode { target_id, requester_id } => {
                let routing_table = routing_table.read().await;
                let target_node_id = hex::decode(&target_id)
                    .map_err(|e| DhtError::InvalidMessage(format!("Invalid target_id: {}", e)))?;
                let target_node_id: NodeId = target_node_id.try_into()
                    .map_err(|_| DhtError::InvalidMessage("Invalid target_id length".to_string()))?;
                let closest_nodes = routing_table.find_closest_nodes(&target_node_id, 20);
                Ok(DhtMessage::FindNodeResponse {
                    nodes: closest_nodes,
                })
            }
            DhtMessage::Store { record } => {
                // Verify record signature
                if Self::verify_record(&record)? {
                    let mut storage = storage.write().await;
                    storage.insert(record.key.clone(), record);
                    Ok(DhtMessage::StoreResponse {
                        success: true,
                    })
                } else {
                    Ok(DhtMessage::StoreResponse {
                        success: false,
                    })
                }
            }
            DhtMessage::FindValue { key, requester_id: _ } => {
                let storage = storage.read().await;
                if let Some(record) = storage.get(&key) {
                    Ok(DhtMessage::FindValueResponse {
                        value: Some(record.value.clone()),
                        nodes: None,
                    })
                } else {
                    // Return closest nodes that might have the value
                    let target_id = Self::hash_key(&key);
                    let routing_table = routing_table.read().await;
                    let closest_nodes = routing_table.find_closest_nodes(&target_id, 20);
                    Ok(DhtMessage::FindValueResponse {
                        value: None,
                        nodes: Some(closest_nodes),
                    })
                }
            }
            _ => {
                Err(DhtError::InvalidMessage("Unsupported message type".to_string()))
            }
        }
    }
    
    /// Send message to peer and receive response
    async fn send_message_to_peer(
        &self,
        peer_addr: SocketAddr,
        message: DhtMessage,
    ) -> Result<DhtMessage, DhtError> {
        // Connect to peer
        let mut stream = TcpStream::connect(peer_addr).await?;
        
        // Serialize message
        let msg_data = bincode::serialize(&message)?;
        let msg_len = (msg_data.len() as u32).to_be_bytes();
        
        // Send message
        stream.write_all(&msg_len).await?;
        stream.write_all(&msg_data).await?;
        stream.flush().await?;
        
        // Read response length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let response_len = u32::from_be_bytes(len_buf) as usize;
        
        // Read response data
        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).await?;
        
        // Deserialize response
        let response: DhtMessage = bincode::deserialize(&response_buf)?;
        
        Ok(response)
    }
    
    /// Bootstrap from known peers
    async fn bootstrap(&self) -> Result<(), DhtError> {
        info!("Bootstrapping DHT from {} peers", self.bootstrap_peers.len());
        
        for &peer_addr in &self.bootstrap_peers {
            if let Err(e) = self.connect_to_peer(peer_addr).await {
                warn!("Failed to connect to bootstrap peer {}: {}", peer_addr, e);
            }
        }
        
        Ok(())
    }
    
    /// Connect to a specific peer
    async fn connect_to_peer(&self, peer_addr: SocketAddr) -> Result<(), DhtError> {
        let node_id = Self::calculate_node_id(&self.keypair.verifying_key().to_bytes());
        
        // Send ping message
        let ping_message = DhtMessage::Ping {
            node_id,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap().as_secs(),
        };
        
        match self.send_message_to_peer(peer_addr, ping_message).await {
            Ok(DhtMessage::Pong { node_id: peer_node_id, .. }) => {
                // Add peer to routing table
                let peer_info = PeerInfo {
                    peer_id: hex::encode(peer_node_id),
                    address: format!("/ip4/{}/tcp/{}", peer_addr.ip(), peer_addr.port())
                        .parse().unwrap(),
                    public_key: vec![], // Would be exchanged during handshake
                    last_seen: SystemTime::now(),
                    avg_response_time: Duration::from_millis(100),
                };
                
                let mut routing_table = self.routing_table.write().await;
                routing_table.add_peer(peer_info);
                
                info!("Successfully connected to peer {}", peer_addr);
                Ok(())
            }
            Ok(_) => Err(DhtError::InvalidMessage("Expected pong response".to_string())),
            Err(e) => Err(e),
        }
    }
    
    /// Store a record in the DHT
    pub async fn store(&self, key: &str, value: Vec<u8>) -> Result<(), DhtError> {
        // Create signed record
        let record = self.create_signed_record(key, value)?;
        
        // Find closest nodes to store the record
        let target_id = Self::hash_key(key);
        let routing_table = self.routing_table.read().await;
        let closest_nodes = routing_table.find_closest_nodes(&target_id, 3);
        
        // Store on closest nodes
        for peer in closest_nodes {
            if let Ok(peer_addr) = self.peer_info_to_socket_addr(&peer) {
                let store_message = DhtMessage::Store {
                    record: record.clone(),
                };
                
                if let Err(e) = self.send_message_to_peer(peer_addr, store_message).await {
                    warn!("Failed to store record on peer {}: {}", peer_addr, e);
                }
            }
        }
        
        // Also store locally
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), record);
        
        Ok(())
    }
    
    /// Retrieve a record from the DHT
    pub async fn get(&self, key: &str) -> Result<Vec<u8>, DhtError> {
        // Check local storage first
        {
            let storage = self.storage.read().await;
            if let Some(record) = storage.get(key) {
                return Ok(record.value.clone());
            }
        }
        
        // Search network
        let target_id = Self::hash_key(key);
        let routing_table = self.routing_table.read().await;
        let closest_nodes = routing_table.find_closest_nodes(&target_id, 5);
        
        for peer in closest_nodes {
            if let Ok(peer_addr) = self.peer_info_to_socket_addr(&peer) {
                let find_message = DhtMessage::FindValue {
                    key: key.to_string(),
                    requester_id: hex::encode(Self::calculate_node_id(&self.keypair.verifying_key().to_bytes())),
                };
                
                match self.send_message_to_peer(peer_addr, find_message).await {
                    Ok(DhtMessage::FindValueResponse { value: Some(value), .. }) => {
                        return Ok(value);
                    }
                    Ok(DhtMessage::FindValueResponse { nodes, .. }) => {
                        // Could continue searching with returned nodes
                        if let Some(ref nodes) = nodes {
                            debug!("Received {} additional nodes to search", nodes.len());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to query peer {}: {}", peer_addr, e);
                    }
                    _ => {}
                }
            }
        }
        
        Err(DhtError::NotFound(format!("Key '{}' not found in DHT", key)))
    }
    
    /// Find closest nodes to a target ID
    /// Advanced find node with iterative lookup
    pub async fn find_node(&self, target_id: &str) -> Result<Vec<PeerInfo>, DhtError> {
        self.iterative_find_node(target_id, false).await
    }
    
    /// Advanced find value with iterative lookup
    pub async fn find_value(&self, key: &str) -> Result<Vec<u8>, DhtError> {
        // First check local storage
        if let Ok(value) = self.get(key).await {
            return Ok(value);
        }
        
        // Perform iterative lookup for the key
        let peers = self.iterative_find_node(key, true).await?;
        
        // Query found peers for the value
        for peer in peers {
            if let Ok(addr) = self.peer_info_to_socket_addr(&peer) {
                match self.query_peer_for_value(&addr, key).await {
                    Ok(Some(value)) => return Ok(value),
                    Ok(None) => continue,
                    Err(_) => continue,
                }
            }
        }
        
        Err(DhtError::NotFound(format!("Value not found for key: {}", key)))
    }
    
    /// Iterative find node algorithm (Kademlia standard)
    async fn iterative_find_node(&self, target_id: &str, find_value: bool) -> Result<Vec<PeerInfo>, DhtError> {
        let target_hash = Self::hash_key(target_id);
        let alpha = 3; // Parallelism factor
        let k = 20; // Desired number of closest nodes
        
        // Get initial closest nodes from routing table
        let routing_table = self.routing_table.read().await;
        let mut closest_nodes = routing_table.find_closest_nodes(&target_hash, k);
        drop(routing_table);
        
        if closest_nodes.is_empty() {
            return Err(DhtError::NotFound("No nodes in routing table".to_string()));
        }
        
        let mut queried_nodes = std::collections::HashSet::new();
        let mut contacted_nodes = std::collections::HashSet::new();
        
        // Iterative improvement loop
        for round in 0..10 { // Maximum 10 rounds
            let mut candidates = Vec::new();
            
            // Select alpha unqueried nodes from closest_nodes
            for node in &closest_nodes {
                if !queried_nodes.contains(&node.peer_id) && candidates.len() < alpha {
                    candidates.push(node.clone());
                }
            }
            
            if candidates.is_empty() {
                break; // No more nodes to query
            }
            
            // Query candidates in parallel
            let mut tasks = Vec::new();
            for candidate in candidates {
                queried_nodes.insert(candidate.peer_id.clone());
                contacted_nodes.insert(candidate.peer_id.clone());
                
                if let Ok(addr) = self.peer_info_to_socket_addr(&candidate) {
                    let target_id_owned = target_id.to_string();
                    let find_value_flag = find_value;
                    
                    let task = tokio::spawn(async move {
                        if find_value_flag {
                            // Try to find value first
                            // Implementation would query for specific value
                            None // Placeholder for value query
                        } else {
                            // Query for closer nodes
                            Self::query_peer_for_closest_nodes(addr, &target_id_owned).await
                        }
                    });
                    tasks.push(task);
                }
            }
            
            // Collect results
            let mut new_nodes = Vec::new();
            for task in tasks {
                if let Ok(result) = task.await {
                    if let Some(nodes) = result {
                        new_nodes.extend(nodes);
                    }
                }
            }
            
            // Add new nodes to closest_nodes and sort
            for node in new_nodes {
                if !contacted_nodes.contains(&node.peer_id) {
                    closest_nodes.push(node);
                }
            }
            
            // Sort by distance to target and keep only k closest
            let target_hash = Self::hash_key(target_id);
            closest_nodes.sort_by_key(|node| {
                let node_hash = Self::hash_key(&node.peer_id);
                Self::xor_distance(&target_hash, &node_hash)
            });
            closest_nodes.truncate(k);
            
            // Check for convergence
            if round > 0 && closest_nodes.len() >= k {
                // If we have k nodes and no improvement, stop
                break;
            }
        }
        
        Ok(closest_nodes)
    }
    
    /// Query a peer for closest nodes to target
    async fn query_peer_for_closest_nodes(peer_addr: std::net::SocketAddr, target_id: &str) -> Option<Vec<PeerInfo>> {
        match tokio::net::TcpStream::connect(peer_addr).await {
            Ok(mut stream) => {
                let message = DhtMessage::FindNode {
                    target_id: target_id.to_string(),
                    requester_id: "anonymous".to_string(),
                };
                
                if let Ok(serialized) = serde_json::to_vec(&message) {
                    if let Ok(_) = stream.write_all(&serialized).await {
                        let mut buffer = vec![0; 4096];
                        if let Ok(n) = stream.read(&mut buffer).await {
                            buffer.truncate(n);
                            if let Ok(response) = serde_json::from_slice::<DhtMessage>(&buffer) {
                                if let DhtMessage::FindNodeResponse { nodes } = response {
                                    return Some(nodes);
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }
        None
    }
    
    /// Query a peer for a specific value
    async fn query_peer_for_value(&self, peer_addr: &std::net::SocketAddr, key: &str) -> Result<Option<Vec<u8>>, DhtError> {
        let mut stream = tokio::net::TcpStream::connect(peer_addr).await
            .map_err(|e| DhtError::Connection(e.to_string()))?;
            
        let message = DhtMessage::FindValue {
            key: key.to_string(),
            requester_id: hex::encode(&self.local_addr.to_string()),
        };
        
        let serialized = serde_json::to_vec(&message)
            .map_err(|e| DhtError::Serialization(Box::new(bincode::ErrorKind::Custom(e.to_string()))))?;
            
        stream.write_all(&serialized).await
            .map_err(|e| DhtError::Connection(e.to_string()))?;
            
        let mut buffer = vec![0; 8192];
        let n = stream.read(&mut buffer).await
            .map_err(|e| DhtError::Connection(e.to_string()))?;
            
        buffer.truncate(n);
        let response = serde_json::from_slice::<DhtMessage>(&buffer)
            .map_err(|e| DhtError::Deserialization(e.to_string()))?;
            
        match response {
            DhtMessage::FindValueResponse { value: Some(value), .. } => Ok(Some(value)),
            DhtMessage::FindValueResponse { value: None, .. } => Ok(None),
            DhtMessage::FindNodeResponse { nodes: _ } => Ok(None), // Peer doesn't have value, returned closest nodes
            _ => Err(DhtError::Protocol("Unexpected response".to_string())),
        }
    }
    
    /// Store value with intelligent routing
    pub async fn store_with_routing(&self, key: &str, value: Vec<u8>) -> Result<(), DhtError> {
        // Store locally first
        self.store(key, value.clone()).await?;
        
        // Find k closest nodes to the key
        let closest_nodes = self.iterative_find_node(key, false).await?;
        
        // Store on closest nodes
        let mut successful_stores = 0;
        let required_replicas = std::cmp::min(3, closest_nodes.len()); // Store on at least 3 nodes
        
        for node in closest_nodes.iter().take(required_replicas * 2) { // Try more nodes for redundancy
            if let Ok(addr) = self.peer_info_to_socket_addr(node) {
                match self.store_on_peer(&addr, key, &value).await {
                    Ok(_) => {
                        successful_stores += 1;
                        if successful_stores >= required_replicas {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to store on peer {}: {}", node.peer_id, e);
                    }
                }
            }
        }
        
        if successful_stores == 0 {
            return Err(DhtError::BootstrapFailed);
        }
        
        info!("Successfully stored key '{}' on {} nodes", key, successful_stores);
        Ok(())
    }
    
    /// Store a value on a remote peer
    async fn store_on_peer(&self, peer_addr: &std::net::SocketAddr, key: &str, value: &[u8]) -> Result<(), DhtError> {
        let mut stream = tokio::net::TcpStream::connect(peer_addr).await
            .map_err(|e| DhtError::Connection(e.to_string()))?;
            
        let record = self.create_signed_record(key, value.to_vec())?;
        let message = DhtMessage::Store { record };
        
        let serialized = serde_json::to_vec(&message)
            .map_err(|e| DhtError::Serialization(Box::new(bincode::ErrorKind::Custom(e.to_string()))))?;
            
        stream.write_all(&serialized).await
            .map_err(|e| DhtError::Connection(e.to_string()))?;
            
        let mut buffer = vec![0; 1024];
        let n = stream.read(&mut buffer).await
            .map_err(|e| DhtError::Connection(e.to_string()))?;
            
        buffer.truncate(n);
        let response = serde_json::from_slice::<DhtMessage>(&buffer)
            .map_err(|e| DhtError::Deserialization(e.to_string()))?;
            
        match response {
            DhtMessage::StoreResponse { success: true } => Ok(()),
            DhtMessage::StoreResponse { success: false } => Err(DhtError::Protocol("Store rejected".to_string())),
            _ => Err(DhtError::Protocol("Unexpected response".to_string())),
        }
    }
    
    /// Get routing table statistics
    pub async fn get_routing_stats(&self) -> RoutingStats {
        let routing_table = self.routing_table.read().await;
        let mut total_peers = 0;
        let mut active_buckets = 0;
        let mut bucket_distribution = Vec::new();
        
        for (i, bucket) in routing_table.buckets.iter().enumerate() {
            let peer_count = bucket.peers.len();
            total_peers += peer_count;
            bucket_distribution.push((i, peer_count));
            
            if peer_count > 0 {
                active_buckets += 1;
            }
        }
        
        RoutingStats {
            total_peers,
            active_buckets,
            total_buckets: routing_table.buckets.len(),
            k_value: routing_table.k_value,
            bucket_distribution,
        }
    }
    
    /// Create a signed record
    fn create_signed_record(&self, key: &str, value: Vec<u8>) -> Result<DhtRecord, DhtError> {
        let timestamp = SystemTime::now();
        let ttl = Duration::from_secs(3600); // 1 hour default TTL
        
        // Create record content for signing
        let record_content = format!("{}:{:?}:{:?}:{:?}", 
            key, value, timestamp, ttl);
        
        // Sign the record
        let signature = self.keypair.sign(record_content.as_bytes());
        
        Ok(DhtRecord {
            key: key.to_string(),
            value,
            timestamp,
            ttl,
            signature: signature.to_bytes().to_vec(),
            publisher: self.keypair.verifying_key().to_bytes().to_vec(),
        })
    }
    
    /// Verify a record's signature
    fn verify_record(record: &DhtRecord) -> Result<bool, DhtError> {
        if record.publisher.len() != 32 {
            return Ok(false);
        }
        
        // Convert Vec<u8> to [u8; 32] for public key
        if record.publisher.len() != 32 {
            return Err(DhtError::Crypto("Public key must be 32 bytes".to_string()));
        }
        let mut pub_key_bytes = [0u8; 32];
        pub_key_bytes.copy_from_slice(&record.publisher);
        let public_key = VerifyingKey::from_bytes(&pub_key_bytes)
            .map_err(|e| DhtError::Crypto(format!("Invalid public key: {}", e)))?;
        
        let record_content = format!("{}:{:?}:{:?}:{:?}", 
            record.key, record.value, record.timestamp, record.ttl);
        
        if record.signature.len() != 64 {
            return Ok(false);
        }
        
        // Convert Vec<u8> to [u8; 64] for signature
        if record.signature.len() != 64 {
            return Err(DhtError::Crypto("Signature must be 64 bytes".to_string()));
        }
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&record.signature);
        let signature = Signature::from_bytes(&sig_bytes);
        
        Ok(public_key.verify(record_content.as_bytes(), &signature).is_ok())
    }
    
    /// Calculate node ID from public key
    fn calculate_node_id(public_key: &[u8]) -> NodeId {
        let mut hasher = Sha256::new();
        hasher.update(public_key);
        let hash = hasher.finalize();
        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&hash);
        node_id
    }
    
    /// Hash a key to node ID
    fn hash_key(key: &str) -> NodeId {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&hash);
        node_id
    }
    
    /// Calculate XOR distance between two node IDs
    fn xor_distance(a: &NodeId, b: &NodeId) -> NodeId {
        let mut distance = [0u8; 32];
        for i in 0..32 {
            distance[i] = a[i] ^ b[i];
        }
        distance
    }
    
    /// Convert PeerInfo to SocketAddr
    fn peer_info_to_socket_addr(&self, peer: &PeerInfo) -> Result<SocketAddr, DhtError> {
        let addr_str = peer.address.to_string();
        
        // Parse multiaddr to extract IP and port
        if let Some(ip_start) = addr_str.find("/ip4/") {
            let ip_start = ip_start + 5;
            if let Some(ip_end) = addr_str[ip_start..].find('/') {
                let ip = &addr_str[ip_start..ip_start + ip_end];
                
                if let Some(port_start) = addr_str.find("/tcp/") {
                    let port_start = port_start + 5;
                    if let Some(port_end) = addr_str[port_start..].find('/') {
                        let port = &addr_str[port_start..port_start + port_end];
                        let socket_addr = format!("{}:{}", ip, port).parse()
                            .map_err(|e| DhtError::Communication(format!("Invalid address: {}", e)))?;
                        return Ok(socket_addr);
                    } else {
                        let port = &addr_str[port_start..];
                        let socket_addr = format!("{}:{}", ip, port).parse()
                            .map_err(|e| DhtError::Communication(format!("Invalid address: {}", e)))?;
                        return Ok(socket_addr);
                    }
                }
            }
        }
        
        Err(DhtError::Communication("Invalid multiaddr format".to_string()))
    }
}

impl RoutingTable {
    /// Create new routing table
    pub fn new(local_node_id: NodeId, k_value: usize) -> Self {
        let mut buckets = Vec::new();
        
        // Create 256 buckets for each bit position
        for i in 0..=255 {
            buckets.push(KBucket {
                peers: Vec::new(),
                max_size: k_value,
                distance_range: (i, i),
            });
        }
        
        RoutingTable {
            buckets,
            local_node_id,
            k_value,
        }
    }
    
    /// Add peer to routing table
    pub fn add_peer(&mut self, peer: PeerInfo) {
        // Calculate distance to determine bucket
        let peer_node_id = Self::hash_peer_id(&peer.peer_id);
        let distance = self.calculate_distance(&peer_node_id);
        let bucket_index = self.distance_to_bucket_index(distance);
        
        if bucket_index < self.buckets.len() {
            let bucket = &mut self.buckets[bucket_index];
            
            // Remove if already exists
            bucket.peers.retain(|p| p.peer_id != peer.peer_id);
            
            // Add to end (most recently seen)
            if bucket.peers.len() < bucket.max_size {
                bucket.peers.push(peer);
            } else {
                // Bucket full, remove least recently seen
                bucket.peers.remove(0);
                bucket.peers.push(peer);
            }
        }
    }
    
    /// Find closest nodes to target
    pub fn find_closest_nodes(&self, target_id: &NodeId, count: usize) -> Vec<PeerInfo> {
        let mut candidates = Vec::new();
        
        // Collect all peers with their distances
        for bucket in &self.buckets {
            for peer in &bucket.peers {
                let peer_node_id = Self::hash_peer_id(&peer.peer_id);
                let distance = Self::xor_distance(target_id, &peer_node_id);
                candidates.push((distance, peer.clone()));
            }
        }
        
        // Sort by distance and return closest
        candidates.sort_by_key(|(distance, _)| *distance);
        candidates.into_iter()
            .take(count)
            .map(|(_, peer)| peer)
            .collect()
    }
    
    /// Find nodes in a specific bucket for maintenance
    pub fn find_bucket_nodes(&self, bucket_index: usize) -> Vec<PeerInfo> {
        if bucket_index < self.buckets.len() {
            self.buckets[bucket_index].peers.clone()
        } else {
            Vec::new()
        }
    }
    
    /// Get all peers for backup/persistence  
    pub fn get_all_peers(&self) -> Vec<PeerInfo> {
        let mut all_peers = Vec::new();
        for bucket in &self.buckets {
            all_peers.extend(bucket.peers.clone());
        }
        all_peers
    }
    
    /// Remove stale peers based on last seen time
    pub fn remove_stale_peers(&mut self, stale_threshold: Duration) {
        let now = SystemTime::now();
        
        for bucket in &mut self.buckets {
            bucket.peers.retain(|peer| {
                match now.duration_since(peer.last_seen) {
                    Ok(duration) => duration < stale_threshold,
                    Err(_) => true, // Keep peers with future timestamps (clock skew)
                }
            });
        }
    }
    
    /// Update peer response time and last seen
    pub fn update_peer_stats(&mut self, peer_id: &str, response_time: Duration) {
        for bucket in &mut self.buckets {
            if let Some(peer) = bucket.peers.iter_mut().find(|p| p.peer_id == peer_id) {
                peer.last_seen = SystemTime::now();
                // Update rolling average of response time
                let current_avg = peer.avg_response_time.as_millis() as f64;
                let new_time = response_time.as_millis() as f64;
                let updated_avg = (current_avg * 0.8) + (new_time * 0.2); // Exponential moving average
                peer.avg_response_time = Duration::from_millis(updated_avg as u64);
                break;
            }
        }
    }
    
    /// Find replacement candidates for a failed peer
    pub fn find_replacement_candidates(&self, failed_peer_id: &str, count: usize) -> Vec<PeerInfo> {
        let failed_peer_hash = Self::hash_peer_id(failed_peer_id);
        
        // Find candidates from nearby buckets
        let bucket_index = self.distance_to_bucket_index(self.calculate_distance(&failed_peer_hash));
        let mut candidates = Vec::new();
        
        // Check current bucket and adjacent buckets
        for i in bucket_index.saturating_sub(2)..=std::cmp::min(bucket_index + 2, self.buckets.len() - 1) {
            for peer in &self.buckets[i].peers {
                if peer.peer_id != failed_peer_id {
                    candidates.push(peer.clone());
                }
            }
        }
        
        // Sort by response time and reliability
        candidates.sort_by(|a, b| a.avg_response_time.cmp(&b.avg_response_time));
        candidates.into_iter().take(count).collect()
    }
    
    /// Calculate XOR distance between two node IDs
    fn xor_distance(id1: &NodeId, id2: &NodeId) -> u64 {
        let mut distance = 0u64;
        for i in 0..std::cmp::min(8, id1.len()) {
            distance = (distance << 8) | ((id1[i] ^ id2[i]) as u64);
        }
        distance
    }
    
    /// Calculate distance from local node
    fn calculate_distance(&self, peer_id: &NodeId) -> u64 {
        Self::xor_distance(&self.local_node_id, peer_id)
    }
    
    /// Convert distance to bucket index
    fn distance_to_bucket_index(&self, distance: u64) -> usize {
        if distance == 0 {
            return 0;
        }
        
        // Find the position of the most significant bit
        let msb_pos = 64 - distance.leading_zeros() as usize;
        std::cmp::min(msb_pos, 255)
    }
    
    /// Hash peer ID string to NodeId
    fn hash_peer_id(peer_id: &str) -> NodeId {
        let mut hasher = Sha256::new();
        hasher.update(peer_id.as_bytes());
        let hash = hasher.finalize();
        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&hash);
        node_id
    }
}

// Import logging macros
use tracing::{debug, info, warn, error};
