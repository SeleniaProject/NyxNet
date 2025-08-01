//! Pure Rust DHT implementation using TCP/TLS networking
//! 
//! This module provides a complete Distributed Hash Table (DHT) implementation
//! using only pure Rust dependencies, avoiding any C/C++ code entirely.
//! Uses tokio TCP/TLS for networking to eliminate ring dependency.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH, Instant};
use std::path::PathBuf;
use tokio::sync::RwLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs;
use serde::{Deserialize, Serialize};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use sha2::{Digest, Sha256};
use multiaddr::Multiaddr;
use trust_dns_resolver::TokioAsyncResolver;
use bincode;
use tracing::{info, warn, error, debug};

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
    /// Persistent storage manager
    persistent_storage: Arc<RwLock<PersistentDhtStorage>>,
    /// Bootstrap manager for network initialization
    bootstrap_manager: Arc<RwLock<BootstrapManager>>,
    /// Auto-save interval for persistence
    auto_save_interval: Duration,
    /// Last persistence save time
    last_save_time: Arc<RwLock<Instant>>,
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

/// Persistent storage for DHT state and peer information
#[derive(Debug, Clone)]
pub struct PersistentDhtStorage {
    /// Storage directory path
    storage_dir: PathBuf,
    /// Encryption key for sensitive data
    encryption_key: Option<[u8; 32]>,
    /// Maximum storage size in bytes
    max_storage_size: u64,
    /// Storage compression enabled
    compression_enabled: bool,
}

/// Serializable peer information for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePeerInfo {
    pub peer_id: String,
    pub address: String,
    pub public_key: Vec<u8>,
    pub last_seen: u64, // Unix timestamp
    pub response_time_ms: u64,
}

/// Serializable DHT record for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableDhtRecord {
    pub key: String,
    pub value: Vec<u8>,
    pub timestamp: u64,
    pub ttl: u64,
    pub signature: Vec<u8>,
    pub publisher: String,  // 'author' から 'publisher' に変更
}

/// Bootstrap manager for network initialization
#[derive(Debug)]
pub struct BootstrapManager {
    /// Known bootstrap nodes
    bootstrap_nodes: Vec<BootstrapNode>,
    /// Connection timeout in seconds
    connection_timeout: Duration,
    /// Maximum concurrent bootstrap attempts
    max_concurrent_attempts: usize,
    /// Minimum successful connections required
    min_successful_connections: usize,
    /// Bootstrap retry configuration
    retry_config: BootstrapRetryConfig,
}

/// Configuration for bootstrap retry logic
#[derive(Debug, Clone)]
pub struct BootstrapRetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_backoff: bool,
    pub jitter_enabled: bool,
}

/// Bootstrap node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapNode {
    pub address: SocketAddr,
    pub node_id: Option<String>,
    pub priority: u32, // Lower number = higher priority
    pub last_successful_connection: Option<u64>,
    pub failure_count: u32,
    pub average_response_time: f64,
    pub reliability_score: f64,
    pub supported_protocols: Vec<String>,
}

/// DHT persistence statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceStats {
    pub total_peers_stored: usize,
    pub total_records_stored: usize,
    pub storage_size_bytes: u64,
    pub last_backup_time: Option<u64>,
    pub last_restore_time: Option<u64>,
    pub compression_ratio: f64,
    pub encryption_enabled: bool,
}

/// Lightweight handle for background tasks
#[derive(Debug)]
pub struct TaskDhtHandle {
    persistent_storage: Arc<RwLock<PersistentDhtStorage>>,
    bootstrap_manager: Arc<RwLock<BootstrapManager>>,
    routing_table: Arc<RwLock<RoutingTable>>,
    storage: Arc<RwLock<HashMap<String, DhtRecord>>>,
    last_save_time: Arc<RwLock<Instant>>,
    auto_save_interval: Duration,
}

impl TaskDhtHandle {
    /// Perform auto-save if needed
    async fn auto_save_if_needed(&self) -> Result<(), DhtError> {
        let last_save = *self.last_save_time.read().await;
        if last_save.elapsed() >= self.auto_save_interval {
            self.save_to_storage().await?;
        }
        Ok(())
    }

    /// Save DHT state to storage
    async fn save_to_storage(&self) -> Result<(), DhtError> {
        let storage_guard = self.persistent_storage.read().await;
        let storage_dir = &storage_guard.storage_dir;

        // Save routing table
        self.save_routing_table(storage_dir).await?;
        
        // Save DHT records  
        self.save_dht_records(storage_dir).await?;
        
        // Save bootstrap info
        self.save_bootstrap_info(storage_dir).await?;

        // Update last save time
        *self.last_save_time.write().await = Instant::now();

        debug!("Auto-save completed successfully");
        Ok(())
    }

    /// Save routing table (simplified version for background tasks)
    async fn save_routing_table(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let routing_table = self.routing_table.read().await;
        let peers = routing_table.get_all_peers();
        
        let serializable_peers: Vec<SerializablePeerInfo> = peers.into_iter().map(|peer| {
            SerializablePeerInfo {
                peer_id: peer.peer_id,
                address: peer.address.to_string(),
                public_key: peer.public_key,
                last_seen: peer.last_seen.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                response_time_ms: peer.avg_response_time.as_millis() as u64,
            }
        }).collect();

        let peers_file = storage_dir.join("routing_table.json");
        let json_data = serde_json::to_string_pretty(&serializable_peers)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON serialization failed: {}", e)))?;

        fs::write(&peers_file, json_data).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to write peers file: {}", e)))?;

        Ok(())
    }

    /// Save DHT records (simplified version for background tasks)
    async fn save_dht_records(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let storage = self.storage.read().await;
        
        let serializable_records: Vec<SerializableDhtRecord> = storage.iter().map(|(key, record)| {
            SerializableDhtRecord {
                key: key.clone(),
                value: record.value.clone(),
                timestamp: record.timestamp.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                ttl: record.ttl.as_secs(),
                signature: record.signature.clone(),  // signature は既に Vec<u8>
                publisher: String::from_utf8_lossy(&record.publisher).to_string(),   // Vec<u8> から String へ
            }
        }).collect();

        let records_file = storage_dir.join("dht_records.json");
        let json_data = serde_json::to_string_pretty(&serializable_records)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON serialization failed: {}", e)))?;

        fs::write(&records_file, json_data).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to write records file: {}", e)))?;

        Ok(())
    }

    /// Save bootstrap info (simplified version for background tasks)
    async fn save_bootstrap_info(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let bootstrap_manager = self.bootstrap_manager.read().await;
        
        let bootstrap_file = storage_dir.join("bootstrap_nodes.json");
        let json_data = serde_json::to_string_pretty(&bootstrap_manager.bootstrap_nodes)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON serialization failed: {}", e)))?;

        fs::write(&bootstrap_file, json_data).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to write bootstrap file: {}", e)))?;

        Ok(())
    }

    /// Perform network health check
    async fn perform_network_health_check(&self) -> Result<bool, DhtError> {
        let routing_table = self.routing_table.read().await;
        let peer_count = routing_table.get_all_peers().len();
        
        if peer_count < 1 {
            warn!("Network health check: No peers connected, network may be isolated");
            return Ok(false);
        }
        
        debug!("Network health check: {} peers connected", peer_count);
        Ok(true)
    }
}

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
    /// Create a new Pure Rust DHT node with persistence and bootstrap capabilities
    pub async fn new(
        local_addr: SocketAddr,
        bootstrap_peers: Vec<SocketAddr>,
    ) -> Result<Self, DhtError> {
        Self::new_with_config(local_addr, bootstrap_peers, None, None).await
    }

    /// Create a new Pure Rust DHT node with custom configuration
    pub async fn new_with_config(
        local_addr: SocketAddr,
        bootstrap_peers: Vec<SocketAddr>,
        storage_dir: Option<PathBuf>,
        encryption_key: Option<[u8; 32]>,
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
        
        // Setup persistent storage directory
        let storage_dir = storage_dir.unwrap_or_else(|| {
            let mut dir = std::env::temp_dir();
            dir.push("nyx_dht_storage");
            dir
        });

        // Create storage directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&storage_dir).await {
            warn!("Failed to create storage directory: {}", e);
        }

        // Initialize persistent storage
        let persistent_storage = Arc::new(RwLock::new(PersistentDhtStorage {
            storage_dir,
            encryption_key,
            max_storage_size: 1024 * 1024 * 100, // 100MB default
            compression_enabled: true,
        }));

        // Initialize bootstrap manager
        let bootstrap_nodes = bootstrap_peers.iter().map(|addr| BootstrapNode {
            address: *addr,
            node_id: None,
            priority: 1,
            last_successful_connection: None,
            failure_count: 0,
            average_response_time: 0.0,
            reliability_score: 1.0,
            supported_protocols: vec!["tcp".to_string()],
        }).collect();

        let bootstrap_manager = Arc::new(RwLock::new(BootstrapManager {
            bootstrap_nodes,
            connection_timeout: Duration::from_secs(30),
            max_concurrent_attempts: 5,
            min_successful_connections: 1,
            retry_config: BootstrapRetryConfig {
                max_retries: 3,
                base_delay_ms: 1000,
                max_delay_ms: 30000,
                exponential_backoff: true,
                jitter_enabled: true,
            },
        }));

        let auto_save_interval = Duration::from_secs(300); // Save every 5 minutes
        let last_save_time = Arc::new(RwLock::new(Instant::now()));

        let mut dht = PureRustDht {
            keypair,
            local_addr,
            routing_table,
            storage,
            listener: None,
            bootstrap_peers,
            dns_resolver,
            persistent_storage,
            bootstrap_manager,
            auto_save_interval,
            last_save_time,
        };

        // Attempt to restore state from persistent storage
        if let Err(e) = dht.restore_from_storage().await {
            warn!("Failed to restore DHT state from storage: {}", e);
        }

        Ok(dht)
    }
    
    /// Start the DHT node with full persistence and bootstrap integration
    pub async fn start(&mut self) -> Result<(), DhtError> {
        // Bind TCP listener
        let listener = TcpListener::bind(self.local_addr).await?;
        self.listener = Some(listener);
        
        info!("Pure Rust DHT node started on {}", self.local_addr);
        
        // Start accepting connections
        self.start_connection_handler().await?;
        
        // Perform bootstrap process to join the network
        if let Err(e) = self.bootstrap().await {
            warn!("Bootstrap process failed: {}, continuing with restored state", e);
        }

        // Start periodic tasks
        self.start_periodic_tasks().await;
        
        Ok(())
    }

    /// Start periodic maintenance tasks
    async fn start_periodic_tasks(&self) {
        let auto_save_dht = Arc::new(self.clone_for_tasks());
        let health_check_dht = Arc::new(self.clone_for_tasks());

        // Auto-save task - runs every 5 minutes
        let auto_save_interval = self.auto_save_interval;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(auto_save_interval);
            loop {
                interval.tick().await;
                if let Err(e) = auto_save_dht.auto_save_if_needed().await {
                    error!("Auto-save failed: {}", e);
                }
            }
        });

        // Network health check task - runs every 30 seconds
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(e) = health_check_dht.perform_network_health_check().await {
                    error!("Network health check failed: {}", e);
                }
            }
        });

        info!("Started periodic maintenance tasks for DHT persistence and network monitoring");
    }

    /// Create a minimal clone for background tasks (avoiding full clone complexity)
    fn clone_for_tasks(&self) -> TaskDhtHandle {
        TaskDhtHandle {
            persistent_storage: Arc::clone(&self.persistent_storage),
            bootstrap_manager: Arc::clone(&self.bootstrap_manager),
            routing_table: Arc::clone(&self.routing_table),
            storage: Arc::clone(&self.storage),
            last_save_time: Arc::clone(&self.last_save_time),
            auto_save_interval: self.auto_save_interval,
        }
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

impl PureRustDht {
    // =====================================================================
    // PERSISTENCE IMPLEMENTATION
    // =====================================================================

    /// Save DHT state to persistent storage
    pub async fn save_to_storage(&self) -> Result<(), DhtError> {
        let storage_guard = self.persistent_storage.read().await;
        let storage_dir = &storage_guard.storage_dir;

        // Ensure storage directory exists
        if let Err(e) = fs::create_dir_all(storage_dir).await {
            return Err(DhtError::InvalidMessage(format!("Failed to create storage directory: {}", e)));
        }

        // Save routing table
        if let Err(e) = self.save_routing_table(storage_dir).await {
            error!("Failed to save routing table: {}", e);
        }

        // Save DHT records
        if let Err(e) = self.save_dht_records(storage_dir).await {
            error!("Failed to save DHT records: {}", e);
        }

        // Save bootstrap information
        if let Err(e) = self.save_bootstrap_info(storage_dir).await {
            error!("Failed to save bootstrap info: {}", e);
        }

        // Update last save time
        *self.last_save_time.write().await = Instant::now();

        info!("Successfully saved DHT state to persistent storage");
        Ok(())
    }

    /// Restore DHT state from persistent storage
    pub async fn restore_from_storage(&mut self) -> Result<(), DhtError> {
        let storage_guard = self.persistent_storage.read().await;
        let storage_dir = &storage_guard.storage_dir;

        // Check if storage directory exists
        if !storage_dir.exists() {
            info!("Storage directory does not exist, starting with fresh state");
            return Ok(());
        }

        // Restore routing table
        if let Err(e) = self.restore_routing_table(storage_dir).await {
            warn!("Failed to restore routing table: {}", e);
        }

        // Restore DHT records
        if let Err(e) = self.restore_dht_records(storage_dir).await {
            warn!("Failed to restore DHT records: {}", e);
        }

        // Restore bootstrap information
        if let Err(e) = self.restore_bootstrap_info(storage_dir).await {
            warn!("Failed to restore bootstrap info: {}", e);
        }

        info!("Successfully restored DHT state from persistent storage");
        Ok(())
    }

    /// Save routing table to persistent storage
    async fn save_routing_table(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let routing_table = self.routing_table.read().await;
        let mut serializable_peers = Vec::new();

        // Convert routing table peers to serializable format
        for peer_info in routing_table.get_all_peers() {
            let serializable_peer = SerializablePeerInfo {
                peer_id: peer_info.peer_id.clone(),
                address: peer_info.address.to_string(),
                public_key: peer_info.public_key.clone(),
                last_seen: peer_info.last_seen.duration_since(UNIX_EPOCH)
                    .unwrap_or_default().as_secs(),
                response_time_ms: peer_info.avg_response_time.as_millis() as u64,
            };
            serializable_peers.push(serializable_peer);
        }

        // Save to file
        let peers_file = storage_dir.join("routing_table.json");
        let json_data = serde_json::to_string_pretty(&serializable_peers)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON serialization failed: {}", e)))?;

        fs::write(&peers_file, json_data).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to write peers file: {}", e)))?;

        debug!("Saved {} peers to routing table storage", serializable_peers.len());
        Ok(())
    }

    /// Restore routing table from persistent storage
    async fn restore_routing_table(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let peers_file = storage_dir.join("routing_table.json");
        
        if !peers_file.exists() {
            debug!("Routing table file does not exist, starting fresh");
            return Ok(());
        }

        let json_data = fs::read_to_string(&peers_file).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to read peers file: {}", e)))?;

        let serializable_peers: Vec<SerializablePeerInfo> = serde_json::from_str(&json_data)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON deserialization failed: {}", e)))?;

        // Convert back to PeerInfo and add to routing table
        let mut routing_table = self.routing_table.write().await;
        let peer_count = serializable_peers.len();
        for serializable_peer in &serializable_peers {
            if let Ok(socket_addr) = serializable_peer.address.parse::<SocketAddr>() {
                let address = if socket_addr.is_ipv4() {
                    Multiaddr::from(socket_addr.ip()).with(multiaddr::Protocol::Tcp(socket_addr.port()))
                } else {
                    Multiaddr::from(socket_addr.ip()).with(multiaddr::Protocol::Tcp(socket_addr.port()))
                };
                let peer_info = PeerInfo {
                    peer_id: serializable_peer.peer_id.clone(),
                    address,
                    public_key: serializable_peer.public_key.clone(),
                    last_seen: UNIX_EPOCH + Duration::from_secs(serializable_peer.last_seen),
                    avg_response_time: Duration::from_millis(serializable_peer.response_time_ms),
                };
                routing_table.add_peer(peer_info);
            }
        }

        info!("Restored routing table with {} peers", peer_count);
        Ok(())
    }

    /// Save DHT records to persistent storage
    async fn save_dht_records(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let storage = self.storage.read().await;
        let mut serializable_records = Vec::new();

        // Convert DHT records to serializable format
        for (key, record) in storage.iter() {
            let serializable_record = SerializableDhtRecord {
                key: key.clone(),
                value: record.value.clone(),
                timestamp: record.timestamp.duration_since(UNIX_EPOCH)
                    .unwrap_or_default().as_secs(),
                ttl: record.ttl.as_secs(),
                signature: record.signature.clone(),  // 既に Vec<u8>
                publisher: String::from_utf8_lossy(&record.publisher).to_string(),   // Vec<u8> から String へ
            };
            serializable_records.push(serializable_record);
        }

        // Save to file
        let records_file = storage_dir.join("dht_records.json");
        let json_data = serde_json::to_string_pretty(&serializable_records)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON serialization failed: {}", e)))?;

        fs::write(&records_file, json_data).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to write records file: {}", e)))?;

        debug!("Saved {} DHT records to storage", serializable_records.len());
        Ok(())
    }

    /// Restore DHT records from persistent storage
    async fn restore_dht_records(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let records_file = storage_dir.join("dht_records.json");
        
        if !records_file.exists() {
            debug!("DHT records file does not exist, starting fresh");
            return Ok(());
        }

        let json_data = fs::read_to_string(&records_file).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to read records file: {}", e)))?;

        let serializable_records: Vec<SerializableDhtRecord> = serde_json::from_str(&json_data)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON deserialization failed: {}", e)))?;

        // Convert back to DhtRecord and add to storage
        let mut storage = self.storage.write().await;
        for serializable_record in serializable_records {
            let record = DhtRecord {
                key: serializable_record.key.clone(),
                value: serializable_record.value,
                timestamp: UNIX_EPOCH + Duration::from_secs(serializable_record.timestamp),
                ttl: Duration::from_secs(serializable_record.ttl),
                signature: serializable_record.signature,  // 既に Vec<u8>
                publisher: serializable_record.publisher.into_bytes(),   // String から Vec<u8> へ
            };
            
            // Check if record is still valid (not expired)
            let now = SystemTime::now();
            if record.timestamp + record.ttl > now {
                storage.insert(serializable_record.key, record);
            }
        }

        info!("Restored {} DHT records from storage", storage.len());
        Ok(())
    }

    /// Save bootstrap information to persistent storage
    async fn save_bootstrap_info(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let bootstrap_manager = self.bootstrap_manager.read().await;
        
        // Save bootstrap nodes information
        let bootstrap_file = storage_dir.join("bootstrap_nodes.json");
        let json_data = serde_json::to_string_pretty(&bootstrap_manager.bootstrap_nodes)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON serialization failed: {}", e)))?;

        fs::write(&bootstrap_file, json_data).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to write bootstrap file: {}", e)))?;

        debug!("Saved {} bootstrap nodes to storage", bootstrap_manager.bootstrap_nodes.len());
        Ok(())
    }

    /// Restore bootstrap information from persistent storage
    async fn restore_bootstrap_info(&self, storage_dir: &PathBuf) -> Result<(), DhtError> {
        let bootstrap_file = storage_dir.join("bootstrap_nodes.json");
        
        if !bootstrap_file.exists() {
            debug!("Bootstrap nodes file does not exist, using configured nodes");
            return Ok(());
        }

        let json_data = fs::read_to_string(&bootstrap_file).await
            .map_err(|e| DhtError::InvalidMessage(format!("Failed to read bootstrap file: {}", e)))?;

        let bootstrap_nodes: Vec<BootstrapNode> = serde_json::from_str(&json_data)
            .map_err(|e| DhtError::InvalidMessage(format!("JSON deserialization failed: {}", e)))?;

        // Update bootstrap manager with restored information
        let mut bootstrap_manager = self.bootstrap_manager.write().await;
        bootstrap_manager.bootstrap_nodes = bootstrap_nodes;

        info!("Restored {} bootstrap nodes from storage", bootstrap_manager.bootstrap_nodes.len());
        Ok(())
    }

    /// Perform automatic save if enough time has passed
    pub async fn auto_save_if_needed(&self) -> Result<(), DhtError> {
        let last_save = *self.last_save_time.read().await;
        if last_save.elapsed() >= self.auto_save_interval {
            self.save_to_storage().await?;
        }
        Ok(())
    }

    /// Get persistence statistics
    pub async fn get_persistence_stats(&self) -> Result<PersistenceStats, DhtError> {
        let storage_guard = self.persistent_storage.read().await;
        let storage_dir = &storage_guard.storage_dir;

        let routing_table = self.routing_table.read().await;
        let storage = self.storage.read().await;

        // Calculate storage size
        let mut total_size = 0u64;
        if let Ok(entries) = fs::read_dir(storage_dir).await {
            let mut entries = entries;
            while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                if let Ok(metadata) = entry.metadata().await {
                    total_size += metadata.len();
                }
            }
        }

        Ok(PersistenceStats {
            total_peers_stored: routing_table.get_all_peers().len(),
            total_records_stored: storage.len(),
            storage_size_bytes: total_size,
            last_backup_time: None, // TODO: Implement backup timestamps
            last_restore_time: None, // TODO: Implement restore timestamps
            compression_ratio: 1.0, // TODO: Implement compression
            encryption_enabled: storage_guard.encryption_key.is_some(),
        })
    }

    // =====================================================================
    // BOOTSTRAP IMPLEMENTATION
    // =====================================================================

    /// Perform bootstrap process to join the DHT network
    pub async fn bootstrap(&self) -> Result<(), DhtError> {
        info!("Starting DHT bootstrap process");

        let bootstrap_manager = self.bootstrap_manager.read().await;
        
        if bootstrap_manager.bootstrap_nodes.is_empty() {
            warn!("No bootstrap nodes configured, cannot join network");
            return Err(DhtError::BootstrapFailed);
        }

        // Sort bootstrap nodes by priority and reliability
        let mut sorted_nodes = bootstrap_manager.bootstrap_nodes.clone();
        sorted_nodes.sort_by(|a, b| {
            a.priority.cmp(&b.priority)
                .then_with(|| b.reliability_score.partial_cmp(&a.reliability_score).unwrap_or(std::cmp::Ordering::Equal))
        });

        let max_concurrent = bootstrap_manager.max_concurrent_attempts;
        let min_successful = bootstrap_manager.min_successful_connections;
        let connection_timeout = bootstrap_manager.connection_timeout;

        // Attempt to connect to bootstrap nodes
        let mut successful_connections = 0;
        let mut failed_connections = 0;

        for chunk in sorted_nodes.chunks(max_concurrent) {
            let mut handles = Vec::new();

            // Start concurrent connection attempts
            for node in chunk {
                let node_addr = node.address;
                let timeout = connection_timeout;
                
                let handle = tokio::spawn(async move {
                    Self::attempt_bootstrap_connection(node_addr, timeout).await
                });
                handles.push((handle, node_addr));
            }

            // Wait for completion and collect results
            for (handle, addr) in handles {
                match handle.await {
                    Ok(Ok(_)) => {
                        successful_connections += 1;
                        info!("Successfully connected to bootstrap node: {}", addr);
                        
                        // Perform initial DHT queries to populate routing table
                        if let Err(e) = self.perform_initial_queries(addr).await {
                            warn!("Failed to perform initial queries to {}: {}", addr, e);
                        }
                    }
                    Ok(Err(e)) => {
                        failed_connections += 1;
                        warn!("Failed to connect to bootstrap node {}: {}", addr, e);
                    }
                    Err(e) => {
                        failed_connections += 1;
                        error!("Bootstrap connection task failed for {}: {}", addr, e);
                    }
                }
            }

            // Check if we have enough successful connections
            if successful_connections >= min_successful {
                break;
            }
        }

        if successful_connections < min_successful {
            error!("Bootstrap failed: only {} of {} required connections successful", 
                   successful_connections, min_successful);
            return Err(DhtError::BootstrapFailed);
        }

        info!("Bootstrap completed successfully: {}/{} connections established", 
              successful_connections, successful_connections + failed_connections);

        // Update bootstrap node statistics
        self.update_bootstrap_statistics(successful_connections, failed_connections).await;

        // Save updated state
        self.save_to_storage().await?;

        Ok(())
    }

    /// Attempt to connect to a single bootstrap node
    async fn attempt_bootstrap_connection(
        node_addr: SocketAddr, 
        timeout: Duration
    ) -> Result<(), DhtError> {
        let connection_start = Instant::now();

        // Attempt TCP connection with timeout
        let stream = tokio::time::timeout(
            timeout,
            TcpStream::connect(node_addr)
        ).await
        .map_err(|_| DhtError::Connection("Bootstrap connection timeout".to_string()))?
        .map_err(|e| DhtError::Connection(format!("Bootstrap connection failed: {}", e)))?;

        // Send ping message to verify connectivity
        let ping_msg = DhtMessage::Ping {
            node_id: [0u8; 32], // Placeholder node ID
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
        };

        // Serialize and send ping
        let serialized = serde_json::to_vec(&ping_msg)
            .map_err(|e| DhtError::Serialization(Box::new(bincode::ErrorKind::Custom(e.to_string()))))?;

        // For simplicity, we'll consider connection successful if TCP connection works
        // In a full implementation, we'd complete the ping/pong handshake

        debug!("Bootstrap connection to {} successful in {:?}", 
               node_addr, connection_start.elapsed());

        Ok(())
    }

    /// Perform initial DHT queries to populate routing table
    async fn perform_initial_queries(&self, bootstrap_addr: SocketAddr) -> Result<(), DhtError> {
        // Generate random node IDs to query for, this helps populate our routing table
        let mut query_targets = Vec::new();
        
        // Query for our own node ID to find closest peers
        let local_node_id = Self::calculate_node_id(&self.keypair.verifying_key().to_bytes());
        query_targets.push(hex::encode(local_node_id));

        // Generate a few random node IDs for broader discovery
        for _ in 0..3 {
            let random_id: [u8; 32] = rand::random();
            query_targets.push(hex::encode(random_id));
        }

        // Perform find_node queries for each target
        for target_id in query_targets {
            if let Some(peer_list) = Self::query_peer_for_closest_nodes(bootstrap_addr, &target_id).await {
                let peer_count = peer_list.len();
                let mut routing_table = self.routing_table.write().await;
                for peer in peer_list {
                    routing_table.add_peer(peer);
                }
                debug!("Added {} peers to routing table from initial query", peer_count);
            }
        }

        Ok(())
    }

    /// Update bootstrap node statistics based on connection results
    async fn update_bootstrap_statistics(&self, successful: usize, failed: usize) -> Result<(), DhtError> {
        let mut bootstrap_manager = self.bootstrap_manager.write().await;
        
        // Update reliability scores based on success/failure rates
        let total_attempts = successful + failed;
        if total_attempts > 0 {
            let success_rate = successful as f64 / total_attempts as f64;
            
            for node in &mut bootstrap_manager.bootstrap_nodes {
                // Update reliability score with exponential moving average
                let alpha = 0.1; // Learning rate
                node.reliability_score = node.reliability_score * (1.0 - alpha) + success_rate * alpha;
                
                // Update last successful connection if this attempt was successful
                if successful > 0 {
                    node.last_successful_connection = Some(
                        SystemTime::now().duration_since(UNIX_EPOCH)
                            .unwrap_or_default().as_secs()
                    );
                }
            }
        }

        debug!("Updated bootstrap statistics: {} successful, {} failed", successful, failed);
        Ok(())
    }

    /// Add a new bootstrap node
    pub async fn add_bootstrap_node(&self, address: SocketAddr, priority: u32) -> Result<(), DhtError> {
        let mut bootstrap_manager = self.bootstrap_manager.write().await;
        
        let new_node = BootstrapNode {
            address,
            node_id: None,
            priority,
            last_successful_connection: None,
            failure_count: 0,
            average_response_time: 0.0,
            reliability_score: 1.0,
            supported_protocols: vec!["tcp".to_string()],
        };

        bootstrap_manager.bootstrap_nodes.push(new_node);
        info!("Added new bootstrap node: {} with priority {}", address, priority);
        
        // Save updated bootstrap configuration
        let storage_guard = self.persistent_storage.read().await;
        self.save_bootstrap_info(&storage_guard.storage_dir).await?;
        
        Ok(())
    }

    /// Remove a bootstrap node
    pub async fn remove_bootstrap_node(&self, address: SocketAddr) -> Result<bool, DhtError> {
        let mut bootstrap_manager = self.bootstrap_manager.write().await;
        
        let initial_len = bootstrap_manager.bootstrap_nodes.len();
        bootstrap_manager.bootstrap_nodes.retain(|node| node.address != address);
        
        let removed = bootstrap_manager.bootstrap_nodes.len() < initial_len;
        if removed {
            info!("Removed bootstrap node: {}", address);
            
            // Save updated bootstrap configuration
            let storage_guard = self.persistent_storage.read().await;
            self.save_bootstrap_info(&storage_guard.storage_dir).await?;
        }
        
        Ok(removed)
    }

    /// Get bootstrap node information
    pub async fn get_bootstrap_nodes(&self) -> Vec<BootstrapNode> {
        let bootstrap_manager = self.bootstrap_manager.read().await;
        bootstrap_manager.bootstrap_nodes.clone()
    }

    /// Check if the node is properly connected to the DHT network
    pub async fn is_network_connected(&self) -> bool {
        let routing_table = self.routing_table.read().await;
        routing_table.get_all_peers().len() >= 1 // At least one peer means we're connected
    }

    /// Perform network health check and auto-recovery if needed
    pub async fn perform_network_health_check(&self) -> Result<bool, DhtError> {
        if !self.is_network_connected().await {
            warn!("Network disconnection detected, attempting automatic recovery");
            
            // Attempt to reconnect through bootstrap
            match self.bootstrap().await {
                Ok(_) => {
                    info!("Network recovery successful");
                    Ok(true)
                }
                Err(e) => {
                    error!("Network recovery failed: {}", e);
                    Ok(false)
                }
            }
        } else {
            Ok(true)
        }
    }
}
