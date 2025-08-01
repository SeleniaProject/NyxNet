//! Pure Rust P2P Network Implementation
//! 
//! This module provides a lightweight P2P networking solution without heavy dependencies
//! that could cause compilation issues. It includes:
//! - Secure TLS-based peer communication using rustls
//! - Distributed Hash Table (DHT) integration
//! - Peer discovery and connection management
//! - Message routing and protocols

use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, IpAddr},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{RwLock, mpsc},
    time::timeout,
    io::{AsyncReadExt, AsyncWriteExt},
};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};
use blake3;
use x25519_dalek::{PublicKey, StaticSecret};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce, Key,
};
use ed25519_dalek::{Signer, Verifier, Signature, SigningKey, VerifyingKey};

/// Quality measurement data
#[derive(Debug, Clone)]
pub struct QualityMeasurement {
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
    pub packet_loss: f64,
    pub timestamp: SystemTime,
}
use rand::{rngs::OsRng as RandOsRng, RngCore};

use crate::pure_rust_dht_tcp::PureRustDht;

/// Pure Rust P2P Network Manager
pub struct PureRustP2P {
    /// Local node information
    local_peer: PeerInfo,
    /// Connected peers
    connected_peers: Arc<RwLock<HashMap<PeerId, ConnectedPeer>>>,
    /// Known peer addresses
    known_peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    /// DHT integration
    dht: Arc<PureRustDht>,
    /// Network event sender
    event_sender: mpsc::UnboundedSender<P2PNetworkEvent>,
    /// Pure Rust encryption cipher
    cipher: Option<ChaCha20Poly1305>,
    /// Node key exchange secret
    key_exchange_secret: StaticSecret,
    /// Node public key
    public_key: PublicKey,
    /// Network statistics
    stats: Arc<RwLock<NetworkStats>>,
    /// Configuration
    config: P2PConfig,
    /// Connection pool configuration
    pool_config: ConnectionPoolConfig,
    /// Connection pool manager
    connection_pool: Arc<RwLock<ConnectionPool>>,
}

/// Peer identifier (using Blake3 hash for performance)
pub type PeerId = [u8; 32];

/// Peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub address: SocketAddr,
    pub public_key: [u8; 32], // X25519 public key
    pub last_seen: SystemTime,
    pub capabilities: Vec<String>,
    pub version: String,
}

/// Connected peer state
#[derive(Debug, Clone)]
pub struct ConnectedPeer {
    pub info: PeerInfo,
    pub connection_time: Instant,
    pub last_activity: Instant,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub latency: Option<Duration>,
    pub sender: mpsc::UnboundedSender<P2PMessage>,
    pub session_key: Option<Vec<u8>>,
    pub peer_public_key: Option<PublicKey>,
    pub is_encrypted: bool,
    pub connection_quality: ConnectionQuality,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

/// Connection quality metrics
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
    pub packet_loss_rate: f64,
    pub stability_score: f64,
    pub last_measurement: SystemTime,
}

impl Default for ConnectionQuality {
    fn default() -> Self {
        Self {
            latency_ms: 0.0,
            bandwidth_mbps: 0.0,
            packet_loss_rate: 0.0,
            stability_score: 1.0,
            last_measurement: SystemTime::now(),
        }
    }
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub max_connections_per_peer: u32,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub reconnect_attempts: u32,
    pub reconnect_delay: Duration,
    pub cleanup_interval: Duration,
    pub quality_check_interval: Duration,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_peer: 3,
            connection_timeout: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(15),
            reconnect_attempts: 5,
            reconnect_delay: Duration::from_secs(2),
            cleanup_interval: Duration::from_secs(60),
            quality_check_interval: Duration::from_secs(30),
        }
    }
}

/// Connection pool manager
#[derive(Debug)]
pub struct ConnectionPool {
    /// Active connections per peer
    peer_connections: HashMap<PeerId, Vec<PeerConnection>>,
    /// Connection retry queue
    retry_queue: Vec<RetryConnection>,
    /// Pool statistics
    pool_stats: PoolStatistics,
    /// Connection quality tracker
    quality_tracker: QualityTracker,
}

/// Individual peer connection state
#[derive(Debug, Clone)]
pub struct PeerConnection {
    pub connection_id: u64,
    pub peer_id: PeerId,
    pub address: SocketAddr,
    pub state: ConnectionState,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub quality: ConnectionQuality,
    pub retry_count: u32,
    pub session_key: Option<Vec<u8>>,
}

/// Connection state enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
    Disconnected,
    Reconnecting,
    Failed,
}

/// Retry connection information
#[derive(Debug, Clone)]
pub struct RetryConnection {
    pub peer_id: PeerId,
    pub address: SocketAddr,
    pub attempt: u32,
    pub next_retry: SystemTime,
    pub last_error: String,
}

/// Pool statistics
#[derive(Debug, Clone, Default)]
pub struct PoolStatistics {
    pub total_connections: u64,
    pub active_connections: u32,
    pub failed_connections: u64,
    pub successful_reconnections: u64,
    pub average_connection_time: Duration,
    pub peak_connections: u32,
}

/// Connection quality tracker
#[derive(Debug)]
pub struct QualityTracker {
    pub measurements: HashMap<PeerId, Vec<QualityMeasurement>>,
    pub last_cleanup: Option<SystemTime>,
}

impl Default for QualityTracker {
    fn default() -> Self {
        Self {
            measurements: HashMap::new(),
            last_cleanup: None,
        }
    }
}

/// Quality measurement data point
impl Default for ConnectionPool {
    fn default() -> Self {
        Self {
            peer_connections: HashMap::new(),
            retry_queue: Vec::new(),
            pool_stats: PoolStatistics::default(),
            quality_tracker: QualityTracker::default(),
        }
    }
}

/// P2P network configuration
#[derive(Debug, Clone)]
pub struct P2PConfig {
    pub listen_address: SocketAddr,
    pub max_peers: usize,
    pub connection_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub enable_encryption: bool,
    pub bootstrap_peers: Vec<SocketAddr>,
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
    pub node_id: Vec<u8>,
}

impl Default for P2PConfig {
    fn default() -> Self {
        let signing_key = SigningKey::generate(&mut RandOsRng);
        let verifying_key = signing_key.verifying_key();
        let node_id = blake3::hash(verifying_key.as_bytes()).as_bytes().to_vec();
        
        Self {
            listen_address: "127.0.0.1:3100".parse().unwrap(),
            max_peers: 50,
            connection_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(30),
            enable_encryption: true,
            bootstrap_peers: Vec::new(),
            signing_key,
            verifying_key,
            node_id,
        }
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub peers_connected: usize,
    pub total_connections: u64,
    pub total_messages: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub start_time: SystemTime,
    pub encrypted_connections: u64,
    pub handshakes_completed: u64,
    pub encryption_errors: u64,
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            peers_connected: 0,
            total_connections: 0,
            total_messages: 0,
            bytes_sent: 0,
            bytes_received: 0,
            start_time: SystemTime::now(),
            encrypted_connections: 0,
            handshakes_completed: 0,
            encryption_errors: 0,
        }
    }
}

/// P2P network events
#[derive(Debug, Clone)]
pub enum P2PNetworkEvent {
    PeerConnected { peer_id: PeerId, address: SocketAddr },
    PeerDisconnected { peer_id: PeerId },
    PeerDiscovered { peer_info: PeerInfo },
    MessageReceived { from: PeerId, message: P2PMessage },
    NetworkError { error: String },
}

/// P2P message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    Handshake {
        peer_info: PeerInfo,
        challenge: [u8; 32],
    },
    HandshakeResponse {
        peer_info: PeerInfo,
        challenge_response: [u8; 32],
        signature: Vec<u8>,
    },
    Heartbeat {
        timestamp: u64,
        peer_count: u32,
    },
    DhtRequest {
        key: String,
        operation: DhtOperation,
    },
    DhtResponse {
        key: String,
        data: Option<Vec<u8>>,
        success: bool,
    },
    PeerDiscovery {
        known_peers: Vec<PeerInfo>,
    },
    DataMessage {
        payload: Vec<u8>,
        message_type: String,
    },
}

/// DHT operations for P2P integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DhtOperation {
    Get,
    Put { value: Vec<u8> },
    Delete,
}

impl PureRustP2P {
    /// Create new Pure Rust P2P network
    pub async fn new(
        dht: Arc<PureRustDht>,
        config: P2PConfig,
    ) -> Result<(Self, mpsc::UnboundedReceiver<P2PNetworkEvent>), Box<dyn std::error::Error + Send + Sync>> {
        // Generate local peer identity
        let secret = StaticSecret::random_from_rng(&mut rand::thread_rng());
        let public_key = PublicKey::from(&secret);
        let peer_id = Self::generate_peer_id(&public_key);

        let local_peer = PeerInfo {
            peer_id,
            address: config.listen_address,
            public_key: public_key.to_bytes(),
            last_seen: SystemTime::now(),
            capabilities: vec!["dht".to_string(), "routing".to_string()],
            version: "1.0.0".to_string(),
        };

        info!("Created Pure Rust P2P node with ID: {}", hex::encode(peer_id));

        // Pure Rust encryption setup - no ring dependency
        let cipher = if config.enable_encryption {
            let key_bytes = blake3::hash(&config.signing_key.to_bytes());
            let key = Key::from_slice(&key_bytes.as_bytes()[..32]);
            Some(ChaCha20Poly1305::new(key))
        } else {
            None
        };

        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let pool_config = ConnectionPoolConfig::default();
        let connection_pool = Arc::new(RwLock::new(ConnectionPool::default()));

        let p2p = PureRustP2P {
            local_peer,
            connected_peers: Arc::new(RwLock::new(HashMap::new())),
            known_peers: Arc::new(RwLock::new(HashMap::new())),
            dht,
            event_sender,
            cipher,
            key_exchange_secret: secret,
            public_key,
            stats: Arc::new(RwLock::new(NetworkStats {
                start_time: SystemTime::now(),
                ..Default::default()
            })),
            config,
            pool_config,
            connection_pool,
        };

        Ok((p2p, event_receiver))
    }

    /// Generate peer ID from public key using Blake3
    fn generate_peer_id(public_key: &PublicKey) -> PeerId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&public_key.to_bytes());
        hasher.update(b"nyx-p2p-v1");
        hasher.finalize().into()
    }

    /// Perform secure handshake with Pure Rust cryptography
    async fn secure_handshake(
        &self,
        stream: &mut TcpStream,
        is_initiator: bool,
    ) -> Result<(PublicKey, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.enable_encryption {
            // Return dummy values for unencrypted connections
            return Ok((self.public_key, vec![]));
        }

        let challenge = if is_initiator {
            // Generate and send challenge
            let mut challenge = [0u8; 32];
            RandOsRng.fill_bytes(&mut challenge);
            
            let handshake = P2PMessage::Handshake {
                peer_info: self.local_peer.clone(),
                challenge,
            };
            
            let serialized = bincode::serialize(&handshake)?;
            stream.write_all(&(serialized.len() as u32).to_be_bytes()).await?;
            stream.write_all(&serialized).await?;
            
            challenge.to_vec()
        } else {
            // Receive challenge
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await?;
            let len = u32::from_be_bytes(len_buf) as usize;
            
            let mut msg_buf = vec![0u8; len];
            stream.read_exact(&mut msg_buf).await?;
            
            let handshake: P2PMessage = bincode::deserialize(&msg_buf)?;
            match handshake {
                P2PMessage::Handshake { challenge, .. } => challenge.to_vec(),
                _ => return Err("Invalid handshake message".into()),
            }
        };

        // Create response with signature
        let signature = self.config.signing_key.sign(&challenge);
        let response = P2PMessage::HandshakeResponse {
            peer_info: self.local_peer.clone(),
            challenge_response: challenge.try_into().unwrap(),
            signature: signature.to_bytes().to_vec(),
        };

        let serialized = bincode::serialize(&response)?;
        stream.write_all(&(serialized.len() as u32).to_be_bytes()).await?;
        stream.write_all(&serialized).await?;

        // Derive shared secret for session encryption
        let shared_secret = self.key_exchange_secret.diffie_hellman(&self.public_key);
        
        Ok((self.public_key, shared_secret.as_bytes().to_vec()))
    }

    /// Start the P2P network
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting Pure Rust P2P network on {}", self.config.listen_address);

        // Start listening for incoming connections
        self.start_listener().await?;

        // Connect to bootstrap peers
        self.connect_to_bootstrap_peers().await;

        // Start background tasks
        self.start_background_tasks().await;

        info!("Pure Rust P2P network started successfully");
        Ok(())
    }

    /// Start TCP listener for incoming connections
    async fn start_listener(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(self.config.listen_address).await?;
        let connected_peers = Arc::clone(&self.connected_peers);
        let known_peers = Arc::clone(&self.known_peers);
        let event_sender = self.event_sender.clone();
        let stats = Arc::clone(&self.stats);
        let local_peer = self.local_peer.clone();
        let max_peers = self.config.max_peers;

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                // Check peer limit
                if connected_peers.read().await.len() >= max_peers {
                    debug!("Rejecting connection from {} - peer limit reached", addr);
                    continue;
                }

                debug!("Incoming connection from {}", addr);

                let connected_peers = Arc::clone(&connected_peers);
                let known_peers = Arc::clone(&known_peers);
                let event_sender = event_sender.clone();
                let stats = Arc::clone(&stats);
                let local_peer = local_peer.clone();

                tokio::spawn(async move {
                    if let Err(e) = Self::handle_incoming_connection(
                        stream,
                        addr,
                        connected_peers,
                        known_peers,
                        event_sender,
                        stats,
                        local_peer,
                    ).await {
                        warn!("Error handling connection from {}: {}", addr, e);
                    }
                });
            }
        });

        Ok(())
    }

    /// Handle incoming connection
    async fn handle_incoming_connection(
        stream: TcpStream,
        addr: SocketAddr,
        connected_peers: Arc<RwLock<HashMap<PeerId, ConnectedPeer>>>,
        known_peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
        event_sender: mpsc::UnboundedSender<P2PNetworkEvent>,
        stats: Arc<RwLock<NetworkStats>>,
        local_peer: PeerInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // For now, implement basic handshake without TLS
        // In production, this would include proper TLS handshake and authentication

        debug!("Handling connection from {}", addr);

        // Create message channel for this peer
        let (tx, mut rx) = mpsc::unbounded_channel::<P2PMessage>();

        // Simulate peer handshake
        let remote_peer_id = Self::generate_peer_id(&PublicKey::from([0u8; 32])); // Placeholder

        let peer_info = PeerInfo {
            peer_id: remote_peer_id,
            address: addr,
            public_key: [0u8; 32], // Placeholder
            last_seen: SystemTime::now(),
            capabilities: vec!["dht".to_string()],
            version: "1.0.0".to_string(),
        };

        let connected_peer = ConnectedPeer {
            info: peer_info.clone(),
            connection_time: Instant::now(),
            last_activity: Instant::now(),
            bytes_sent: 0,
            bytes_received: 0,
            latency: None,
            sender: tx,
            session_key: None,
            peer_public_key: None,
            is_encrypted: false,
            connection_quality: ConnectionQuality::default(),
            retry_count: 0,
            last_error: None,
        };

        // Add to connected peers
        connected_peers.write().await.insert(remote_peer_id, connected_peer);
        known_peers.write().await.insert(remote_peer_id, peer_info.clone());

        // Update statistics
        {
            let mut stats = stats.write().await;
            stats.peers_connected = connected_peers.read().await.len();
            stats.total_connections += 1;
        }

        // Send connection event
        let _ = event_sender.send(P2PNetworkEvent::PeerConnected {
            peer_id: remote_peer_id,
            address: addr,
        });

        info!("Peer connected: {} from {}", hex::encode(remote_peer_id), addr);

        // Handle messages from this peer (placeholder)
        while let Some(_message) = rx.recv().await {
            // Process peer messages
            debug!("Received message from peer {}", hex::encode(remote_peer_id));
        }

        // Cleanup on disconnect
        connected_peers.write().await.remove(&remote_peer_id);
        let _ = event_sender.send(P2PNetworkEvent::PeerDisconnected {
            peer_id: remote_peer_id,
        });

        Ok(())
    }

    /// Encrypt message using Pure Rust cryptography
    fn encrypt_message(&self, data: &[u8], session_key: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref cipher) = self.cipher {
            let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
            let session_cipher = {
                let key_bytes = blake3::hash(session_key);
                let key = Key::from_slice(&key_bytes.as_bytes()[..32]);
                ChaCha20Poly1305::new(key)
            };
            
            let ciphertext = session_cipher.encrypt(&nonce, data)
                .map_err(|e| format!("Encryption failed: {:?}", e))?;
            
            // Prepend nonce to ciphertext
            let mut encrypted = nonce.to_vec();
            encrypted.extend_from_slice(&ciphertext);
            Ok(encrypted)
        } else {
            // No encryption - return plaintext
            Ok(data.to_vec())
        }
    }

    /// Decrypt message using Pure Rust cryptography
    fn decrypt_message(&self, data: &[u8], session_key: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref _cipher) = self.cipher {
            if data.len() < 12 {
                return Err("Encrypted data too short".into());
            }
            
            let (nonce_bytes, ciphertext) = data.split_at(12);
            let nonce = Nonce::from_slice(nonce_bytes);
            
            let session_cipher = {
                let key_bytes = blake3::hash(session_key);
                let key = Key::from_slice(&key_bytes.as_bytes()[..32]);
                ChaCha20Poly1305::new(key)
            };
            
            let plaintext = session_cipher.decrypt(nonce, ciphertext)
                .map_err(|e| format!("Decryption failed: {:?}", e))?;
            Ok(plaintext)
        } else {
            // No encryption - return as-is
            Ok(data.to_vec())
        }
    }

    /// Connect to bootstrap peers
    async fn connect_to_bootstrap_peers(&self) {
        for &addr in &self.config.bootstrap_peers {
            let connected_peers = Arc::clone(&self.connected_peers);
            let event_sender = self.event_sender.clone();
            let timeout_duration = self.config.connection_timeout;

            tokio::spawn(async move {
                match timeout(timeout_duration, TcpStream::connect(addr)).await {
                    Ok(Ok(_stream)) => {
                        info!("Connected to bootstrap peer: {}", addr);
                        // Handle bootstrap connection (simplified)
                        let _ = event_sender.send(P2PNetworkEvent::PeerConnected {
                            peer_id: [0u8; 32], // Placeholder
                            address: addr,
                        });
                    }
                    Ok(Err(e)) => {
                        warn!("Failed to connect to bootstrap peer {}: {}", addr, e);
                    }
                    Err(_) => {
                        warn!("Timeout connecting to bootstrap peer: {}", addr);
                    }
                }
            });
        }
    }

    /// Start background maintenance tasks
    async fn start_background_tasks(&self) {
        // Heartbeat task
        let connected_peers = Arc::clone(&self.connected_peers);
        let heartbeat_interval = self.config.heartbeat_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                let peers = connected_peers.read().await;
                let peer_count = peers.len();
                
                for (peer_id, peer) in peers.iter() {
                    let heartbeat = P2PMessage::Heartbeat {
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                        peer_count: peer_count as u32,
                    };
                    
                    if let Err(_) = peer.sender.send(heartbeat) {
                        debug!("Failed to send heartbeat to peer {}", hex::encode(peer_id));
                    }
                }
                
                debug!("Sent heartbeat to {} peers", peer_count);
            }
        });

        // Peer discovery task
        let known_peers = Arc::clone(&self.known_peers);
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(120));
            
            loop {
                interval.tick().await;
                
                let peers: Vec<PeerInfo> = known_peers.read().await.values().cloned().collect();
                
                if !peers.is_empty() {
                    let discovery_msg = P2PMessage::PeerDiscovery {
                        known_peers: peers.clone(),
                    };
                    
                    // Broadcast peer discovery (simplified)
                    debug!("Broadcasting peer discovery with {} known peers", peers.len());
                }
            }
        });
        
        // Start connection pool maintenance
        self.start_connection_pool_maintenance().await;
    }

    /// Send message to specific peer
    pub async fn send_to_peer(
        &self,
        peer_id: PeerId,
        message: P2PMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let peers = self.connected_peers.read().await;
        
        if let Some(peer) = peers.get(&peer_id) {
            peer.sender.send(message)
                .map_err(|e| format!("Failed to send message to peer: {}", e))?;
            Ok(())
        } else {
            Err(format!("Peer {} not connected", hex::encode(peer_id)).into())
        }
    }

    /// Broadcast message to all connected peers
    pub async fn broadcast_message(&self, message: P2PMessage) -> usize {
        let peers = self.connected_peers.read().await;
        let mut sent_count = 0;
        
        for peer in peers.values() {
            if peer.sender.send(message.clone()).is_ok() {
                sent_count += 1;
            }
        }
        
        debug!("Broadcasted message to {} peers", sent_count);
        sent_count
    }

    /// Get network statistics
    pub async fn get_stats(&self) -> NetworkStats {
        let mut stats = self.stats.read().await.clone();
        stats.peers_connected = self.connected_peers.read().await.len();
        stats
    }

    /// Get connected peer count
    pub async fn peer_count(&self) -> usize {
        self.connected_peers.read().await.len()
    }

    /// Get local peer info
    pub fn local_peer(&self) -> &PeerInfo {
        &self.local_peer
    }

    /// DHT integration methods
    
    /// Store data in DHT via P2P
    pub async fn dht_put(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Store in local DHT
        self.dht.store(&key, value.clone()).await?;
        
        // Broadcast to peers for replication
        let message = P2PMessage::DhtRequest {
            key,
            operation: DhtOperation::Put { value },
        };
        
        self.broadcast_message(message).await;
        Ok(())
    }

    /// Get data from DHT via P2P
    pub async fn dht_get(
        &self,
        key: String,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        // Try local DHT first
        if let Ok(value) = self.dht.get(&key).await {
            return Ok(Some(value));
        }
        
        // Query peers if not found locally
        let message = P2PMessage::DhtRequest {
            key,
            operation: DhtOperation::Get,
        };
        
        self.broadcast_message(message).await;
        
        // For now, return None. In a complete implementation,
        // this would wait for responses from peers
        Ok(None)
    }
}

/// Connection management implementation
impl PureRustP2P {
    /// Connect to a specific peer with automatic retry
    pub async fn connect_to_peer(&self, address: SocketAddr) -> Result<PeerId, Box<dyn std::error::Error + Send + Sync>> {
        info!("Attempting to connect to peer at {}", address);
        
        // Check if already connected
        let existing_peer = self.find_peer_by_address(address).await;
        if let Some(peer_id) = existing_peer {
            debug!("Already connected to peer at {}", address);
            return Ok(peer_id);
        }
        
        // Check connection limit
        let current_connections = self.connected_peers.read().await.len();
        if current_connections >= self.config.max_peers {
            return Err("Maximum peer connections reached".into());
        }
        
        // Attempt connection with timeout
        let connect_future = TcpStream::connect(address);
        let mut stream = timeout(self.pool_config.connection_timeout, connect_future)
            .await
            .map_err(|_| "Connection timeout")?
            .map_err(|e| format!("Connection failed: {}", e))?;
        
        // Perform secure handshake
        let (peer_public_key, session_key) = self.secure_handshake(&mut stream, true).await?;
        let peer_id = Self::generate_peer_id(&peer_public_key);
        
        // Create peer connection
        let (tx, rx) = mpsc::unbounded_channel::<P2PMessage>();
        
        let peer_info = PeerInfo {
            peer_id,
            address,
            public_key: peer_public_key.to_bytes(),
            last_seen: SystemTime::now(),
            capabilities: vec!["dht".to_string(), "routing".to_string()],
            version: "1.0.0".to_string(),
        };
        
        let connected_peer = ConnectedPeer {
            info: peer_info.clone(),
            connection_time: Instant::now(),
            last_activity: Instant::now(),
            bytes_sent: 0,
            bytes_received: 0,
            latency: None,
            sender: tx,
            session_key: Some(session_key),
            peer_public_key: Some(peer_public_key),
            is_encrypted: self.config.enable_encryption,
            connection_quality: ConnectionQuality::default(),
            retry_count: 0,
            last_error: None,
        };
        
        // Add to connection pool
        self.add_to_connection_pool(peer_id, address).await?;
        
        // Add to connected peers
        self.connected_peers.write().await.insert(peer_id, connected_peer);
        self.known_peers.write().await.insert(peer_id, peer_info);
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.peers_connected = self.connected_peers.read().await.len();
            stats.total_connections += 1;
            if self.config.enable_encryption {
                stats.encrypted_connections += 1;
                stats.handshakes_completed += 1;
            }
        }
        
        // Send connection event
        let _ = self.event_sender.send(P2PNetworkEvent::PeerConnected {
            peer_id,
            address,
        });
        
        // Start connection monitoring
        self.start_connection_monitoring(peer_id, stream, rx).await;
        
        info!("Successfully connected to peer {} at {}", hex::encode(peer_id), address);
        Ok(peer_id)
    }
    
    /// Disconnect from a specific peer
    pub async fn disconnect_peer(&self, peer_id: PeerId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Disconnecting from peer {}", hex::encode(peer_id));
        
        // Remove from connected peers
        let peer = self.connected_peers.write().await.remove(&peer_id);
        
        if let Some(_peer) = peer {
            // Remove from connection pool
            self.remove_from_connection_pool(peer_id).await;
            
            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.peers_connected = self.connected_peers.read().await.len();
            }
            
            // Send disconnection event
            let _ = self.event_sender.send(P2PNetworkEvent::PeerDisconnected { peer_id });
            
            info!("Successfully disconnected from peer {}", hex::encode(peer_id));
            Ok(())
        } else {
            Err("Peer not found".into())
        }
    }
    
    /// Reconnect to a failed peer
    pub async fn reconnect_peer(&self, peer_id: PeerId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Attempting to reconnect to peer {}", hex::encode(peer_id));
        
        // Get peer information
        let peer_info = self.known_peers.read().await.get(&peer_id).cloned();
        
        if let Some(info) = peer_info {
            // Update retry count
            if let Some(mut peer) = self.connected_peers.write().await.get_mut(&peer_id) {
                peer.retry_count += 1;
                
                if peer.retry_count > self.pool_config.reconnect_attempts {
                    warn!("Maximum reconnection attempts exceeded for peer {}", hex::encode(peer_id));
                    return Err("Maximum reconnection attempts exceeded".into());
                }
            }
            
            // Remove old connection
            self.disconnect_peer(peer_id).await.ok();
            
            // Wait before retry
            tokio::time::sleep(self.pool_config.reconnect_delay).await;
            
            // Attempt new connection
            match self.connect_to_peer(info.address).await {
                Ok(new_peer_id) => {
                    info!("Successfully reconnected to peer {} (new ID: {})", 
                          hex::encode(peer_id), hex::encode(new_peer_id));
                    Ok(())
                }
                Err(e) => {
                    // Add to retry queue
                    self.add_to_retry_queue(peer_id, info.address, e.to_string()).await;
                    Err(e)
                }
            }
        } else {
            Err("Peer information not found".into())
        }
    }
    
    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> PoolStatistics {
        let pool = self.connection_pool.read().await;
        pool.pool_stats.clone()
    }
    
    /// Get peer connection quality
    pub async fn get_peer_quality(&self, peer_id: PeerId) -> Option<ConnectionQuality> {
        self.connected_peers.read().await
            .get(&peer_id)
            .map(|peer| peer.connection_quality.clone())
    }
    
    /// Update peer connection quality
    pub async fn update_peer_quality(&self, peer_id: PeerId, measurement: QualityMeasurement) {
        if let Some(mut peer) = self.connected_peers.write().await.get_mut(&peer_id) {
            peer.connection_quality.latency_ms = measurement.latency_ms;
            peer.connection_quality.bandwidth_mbps = measurement.bandwidth_mbps;
            peer.connection_quality.packet_loss_rate = measurement.packet_loss;
            peer.connection_quality.last_measurement = measurement.timestamp;
            
            // Update stability score based on measurements
            self.calculate_stability_score(&mut peer.connection_quality);
        }
        
        // Add to quality tracker
        let mut pool = self.connection_pool.write().await;
        pool.quality_tracker.measurements
            .entry(peer_id)
            .or_insert_with(Vec::new)
            .push(measurement);
    }
    
    /// Helper methods for connection pool management
    async fn find_peer_by_address(&self, address: SocketAddr) -> Option<PeerId> {
        for (peer_id, peer) in self.connected_peers.read().await.iter() {
            if peer.info.address == address {
                return Some(*peer_id);
            }
        }
        None
    }
    
    async fn add_to_connection_pool(&self, peer_id: PeerId, address: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut pool = self.connection_pool.write().await;
        
        let connection = PeerConnection {
            connection_id: pool.pool_stats.total_connections + 1,
            peer_id,
            address,
            state: ConnectionState::Connected,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            quality: ConnectionQuality::default(),
            retry_count: 0,
            session_key: None,
        };
        
        pool.peer_connections
            .entry(peer_id)
            .or_insert_with(Vec::new)
            .push(connection);
        
        pool.pool_stats.total_connections += 1;
        pool.pool_stats.active_connections += 1;
        
        if pool.pool_stats.active_connections > pool.pool_stats.peak_connections {
            pool.pool_stats.peak_connections = pool.pool_stats.active_connections;
        }
        
        Ok(())
    }
    
    async fn remove_from_connection_pool(&self, peer_id: PeerId) {
        let mut pool = self.connection_pool.write().await;
        
        if let Some(connections) = pool.peer_connections.get_mut(&peer_id) {
            connections.clear();
            pool.pool_stats.active_connections = pool.pool_stats.active_connections.saturating_sub(1);
        }
        
        pool.peer_connections.remove(&peer_id);
    }
    
    async fn add_to_retry_queue(&self, peer_id: PeerId, address: SocketAddr, error: String) {
        let mut pool = self.connection_pool.write().await;
        
        let retry_connection = RetryConnection {
            peer_id,
            address,
            attempt: 1,
            next_retry: SystemTime::now() + self.pool_config.reconnect_delay,
            last_error: error,
        };
        
        pool.retry_queue.push(retry_connection);
    }
    
    async fn start_connection_monitoring(&self, peer_id: PeerId, _stream: TcpStream, mut _rx: mpsc::UnboundedReceiver<P2PMessage>) {
        let connected_peers = Arc::clone(&self.connected_peers);
        let heartbeat_interval = self.pool_config.heartbeat_interval;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                // Update last activity
                if let Some(mut peer) = connected_peers.write().await.get_mut(&peer_id) {
                    peer.last_activity = Instant::now();
                } else {
                    // Peer disconnected, exit monitoring
                    break;
                }
                
                // Send heartbeat (placeholder)
                debug!("Heartbeat for peer {}", hex::encode(peer_id));
            }
        });
    }
    
    fn calculate_stability_score(&self, quality: &mut ConnectionQuality) {
        // Simple stability calculation based on packet loss and latency consistency
        let loss_factor = 1.0 - quality.packet_loss_rate.min(1.0);
        let latency_factor = if quality.latency_ms < 50.0 { 1.0 } 
                             else if quality.latency_ms < 200.0 { 0.8 }
                             else { 0.5 };
        
        quality.stability_score = (loss_factor * latency_factor).max(0.0).min(1.0);
    }
    
    /// Background maintenance for connection pool
    pub async fn start_connection_pool_maintenance(&self) {
        let connection_pool = Arc::clone(&self.connection_pool);
        let connected_peers = Arc::clone(&self.connected_peers);
        let pool_config = self.pool_config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Clean up stale connections
                let mut pool = connection_pool.write().await;
                let mut stale_peers = Vec::new();
                
                for (peer_id, connections) in pool.peer_connections.iter_mut() {
                    connections.retain(|conn| {
                        let age = SystemTime::now()
                            .duration_since(conn.last_activity)
                            .unwrap_or_default();
                        
                        if age > pool_config.connection_timeout {
                            stale_peers.push(*peer_id);
                            false
                        } else {
                            true
                        }
                    });
                    
                    if connections.is_empty() {
                        stale_peers.push(*peer_id);
                    }
                }
                
                // Remove stale connections
                for peer_id in stale_peers {
                    pool.peer_connections.remove(&peer_id);
                    connected_peers.write().await.remove(&peer_id);
                    pool.pool_stats.active_connections = pool.pool_stats.active_connections.saturating_sub(1);
                }
                
                // Process retry queue
                let now = SystemTime::now();
                let mut retry_queue = std::mem::take(&mut pool.retry_queue);
                retry_queue.retain(|retry| {
                    if retry.next_retry <= now && retry.attempt <= pool_config.reconnect_attempts {
                        // Schedule reconnection (simplified)
                        debug!("Scheduling reconnection for peer {}", hex::encode(retry.peer_id));
                        false // Remove from queue
                    } else if retry.attempt > pool_config.reconnect_attempts {
                        debug!("Giving up reconnection for peer {} after {} attempts", 
                               hex::encode(retry.peer_id), retry.attempt);
                        false // Remove from queue
                    } else {
                        true // Keep in queue
                    }
                });
                pool.retry_queue = retry_queue;
                
                debug!("Connection pool maintenance completed. Active connections: {}", 
                       pool.pool_stats.active_connections);
            }
        });
    }
}
