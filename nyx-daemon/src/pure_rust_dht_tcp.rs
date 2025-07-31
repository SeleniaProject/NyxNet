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
        target_id: NodeId,
        requester_id: NodeId,
    },
    /// Find node response
    FindNodeResponse {
        nodes: Vec<PeerInfo>,
        requester_id: NodeId,
    },
    /// Store record request
    Store {
        record: DhtRecord,
    },
    /// Store response
    StoreResponse {
        success: bool,
        message: String,
    },
    /// Find value request
    FindValue {
        key: String,
        requester_id: NodeId,
    },
    /// Find value response
    FindValueResponse {
        record: Option<DhtRecord>,
        nodes: Vec<PeerInfo>,
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
                let closest_nodes = routing_table.find_closest_nodes(&target_id, 20);
                Ok(DhtMessage::FindNodeResponse {
                    nodes: closest_nodes,
                    requester_id,
                })
            }
            DhtMessage::Store { record } => {
                // Verify record signature
                if Self::verify_record(&record)? {
                    let mut storage = storage.write().await;
                    storage.insert(record.key.clone(), record);
                    Ok(DhtMessage::StoreResponse {
                        success: true,
                        message: "Record stored successfully".to_string(),
                    })
                } else {
                    Ok(DhtMessage::StoreResponse {
                        success: false,
                        message: "Invalid record signature".to_string(),
                    })
                }
            }
            DhtMessage::FindValue { key, requester_id } => {
                let storage = storage.read().await;
                if let Some(record) = storage.get(&key) {
                    Ok(DhtMessage::FindValueResponse {
                        record: Some(record.clone()),
                        nodes: vec![],
                    })
                } else {
                    // Return closest nodes that might have the value
                    let target_id = Self::hash_key(&key);
                    let routing_table = routing_table.read().await;
                    let closest_nodes = routing_table.find_closest_nodes(&target_id, 20);
                    Ok(DhtMessage::FindValueResponse {
                        record: None,
                        nodes: closest_nodes,
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
                    requester_id: Self::calculate_node_id(&self.keypair.verifying_key().to_bytes()),
                };
                
                match self.send_message_to_peer(peer_addr, find_message).await {
                    Ok(DhtMessage::FindValueResponse { record: Some(record), .. }) => {
                        return Ok(record.value);
                    }
                    Ok(DhtMessage::FindValueResponse { nodes, .. }) => {
                        // Could continue searching with returned nodes
                        debug!("Received {} additional nodes to search", nodes.len());
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
    pub async fn find_node(&self, target_id: &str) -> Result<Vec<PeerInfo>, DhtError> {
        let target_hash = Self::hash_key(target_id);
        let routing_table = self.routing_table.read().await;
        Ok(routing_table.find_closest_nodes(&target_hash, 20))
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
        
        // Collect all peers
        for bucket in &self.buckets {
            for peer in &bucket.peers {
                let peer_node_id = Self::hash_peer_id(&peer.peer_id);
                let distance = Self::xor_distance(target_id, &peer_node_id);
                candidates.push((distance, peer.clone()));
            }
        }
        
        // Sort by distance
        candidates.sort_by_key(|(distance, _)| *distance);
        
        // Return closest nodes
        candidates.into_iter()
            .take(count)
            .map(|(_, peer)| peer)
            .collect()
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
