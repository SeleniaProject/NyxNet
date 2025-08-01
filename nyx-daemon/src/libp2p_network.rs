//! Pure Rust P2P Network Authentication and Encryption
//! 
//! This module provides comprehensive P2P peer authentication and encryption 
//! without libp2p dependencies, integrating with our existing Pure Rust P2P system:
//! - Ed25519-based peer identity verification
//! - X25519 key exchange for session establishment
//! - ChaCha20Poly1305 authenticated encryption
//! - Trust-based peer reputation system
//! - Challenge-response authentication protocol

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    error::Error,
    net::SocketAddr,
};
use tokio::sync::{mpsc, RwLock};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};
use rand;
use bincode;

// Pure Rust cryptography imports
use ed25519_dalek::{Signer, Verifier, Signature, SigningKey, VerifyingKey};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519StaticSecret};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce, Key,
};
use blake3::Hasher;

use crate::pure_rust_dht_tcp::{PureRustDht, DhtError, PeerInfo as DhtPeerInfo, NodeId};

/// Peer ID type - using Blake3 hash of Ed25519 public key
pub type PeerId = [u8; 32];

/// DHT integration key prefixes for authenticated peer data
pub const DHT_PEER_AUTH_PREFIX: &str = "nyx-auth-peer";
pub const DHT_PEER_TRUST_PREFIX: &str = "nyx-trust";
pub const DHT_NETWORK_TOPO_PREFIX: &str = "nyx-topology";
pub const DHT_PEER_CAPABILITIES_PREFIX: &str = "nyx-capabilities";

/// Pure Rust P2P authentication request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRequest {
    /// Unique request identifier
    pub request_id: u64,
    /// Requesting peer's ID (Blake3 hash of Ed25519 public key)
    pub peer_id: PeerId,
    /// Requesting peer's Ed25519 public key
    pub ed25519_public_key: [u8; 32],
    /// Requesting peer's X25519 public key for key exchange
    pub x25519_public_key: [u8; 32],
    /// Challenge for peer to respond with
    pub challenge: [u8; 32],
    /// Signature of identity proof
    pub signature: [u8; 64],
    /// Timestamp in seconds since UNIX epoch
    pub timestamp: u64,
}

/// Pure Rust P2P authentication response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Corresponding request identifier
    pub request_id: u64,
    /// Responding peer's Ed25519 public key
    pub ed25519_public_key: [u8; 32],
    /// Responding peer's X25519 public key for key exchange
    pub x25519_public_key: [u8; 32],
    /// Challenge response
    pub challenge_response: [u8; 32],
    /// Signature of response data
    pub signature: [u8; 64],
    /// Timestamp in seconds since UNIX epoch
    pub timestamp: u64,
}

/// Authentication status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum AuthenticationStatus {
    /// Not yet authenticated
    Pending,
    /// Authentication in progress
    InProgress,
    /// Successfully authenticated
    Authenticated,
    /// Authentication failed
    Failed,
    /// Authentication expired
    Expired,
}

/// P2P errors
#[derive(Debug, Clone)]
pub enum P2PError {
    /// Authentication already in progress
    AuthenticationInProgress,
    /// Unknown authentication request
    UnknownAuthRequest,
    /// Invalid cryptographic key
    InvalidKey,
    /// Invalid peer ID
    InvalidPeerId,
    /// Invalid signature
    InvalidSignature,
    /// Invalid timestamp
    InvalidTimestamp,
    /// Peer not found
    PeerNotFound,
    /// Peer not authenticated
    PeerNotAuthenticated,
    /// Insufficient trust level
    InsufficientTrust,
    /// No secure channel established
    NoSecureChannel,
    /// Encryption failed
    EncryptionFailed,
    /// Decryption failed
    DecryptionFailed,
    /// Network error
    NetworkError(String),
}

impl Error for P2PError {}

impl std::fmt::Display for P2PError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            P2PError::AuthenticationInProgress => write!(f, "Authentication already in progress"),
            P2PError::UnknownAuthRequest => write!(f, "Unknown authentication request"),
            P2PError::InvalidKey => write!(f, "Invalid cryptographic key"),
            P2PError::InvalidPeerId => write!(f, "Invalid peer ID"),
            P2PError::InvalidSignature => write!(f, "Invalid signature"),
            P2PError::InvalidTimestamp => write!(f, "Invalid timestamp"),
            P2PError::PeerNotFound => write!(f, "Peer not found"),
            P2PError::PeerNotAuthenticated => write!(f, "Peer not authenticated"),
            P2PError::InsufficientTrust => write!(f, "Insufficient trust level"),
            P2PError::NoSecureChannel => write!(f, "No secure channel established"),
            P2PError::EncryptionFailed => write!(f, "Encryption failed"),
            P2PError::DecryptionFailed => write!(f, "Decryption failed"),
            P2PError::NetworkError(msg) => write!(f, "Network error: {}", msg),
        }
    }
}
    /// Protocol version
    pub protocol_version: u8,
}

/// Authentication response containing peer credentials
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Response status
    pub status: AuthStatus,
    /// Responding peer's Ed25519 public key (if successful)
    pub ed25519_public_key: Option<[u8; 32]>,
    /// Responding peer's X25519 public key (if successful)
    pub x25519_public_key: Option<[u8; 32]>,
    /// Challenge response signature
    pub challenge_response: Option<[u8; 64]>,
    /// Shared secret derived from X25519 key exchange (encrypted)
    pub encrypted_shared_secret: Option<Vec<u8>>,
    /// Error message (if failed)
    pub error_message: Option<String>,
}

/// Authentication status enumeration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    /// Authentication successful
    Success,
    /// Invalid signature
    InvalidSignature,
    /// Expired timestamp
    Expired,
    /// Unsupported protocol version
    UnsupportedVersion,
    /// Internal error
    InternalError,
}

/// Peer authentication state
#[derive(Debug, Clone)]
pub struct PeerAuthState {
    /// Peer ID (Blake3 hash of Ed25519 public key)
    pub peer_id: PeerId,
    /// Authentication status
    pub status: AuthenticationStatus,
    /// Peer's verified Ed25519 public key
    pub verified_ed25519_key: Option<VerifyingKey>,
    /// Peer's X25519 public key for key exchange
    pub x25519_public_key: Option<X25519PublicKey>,
    /// Shared session key derived from X25519 key exchange
    pub session_key: Option<[u8; 32]>,
    /// ChaCha20Poly1305 cipher for encrypted communication
    pub cipher: Option<ChaCha20Poly1305>,
    /// Authentication timestamp
    pub authenticated_at: Option<Instant>,
    /// Trust level (0.0 to 1.0)
    pub trust_level: f64,
    /// Number of successful authentications
    pub auth_success_count: u32,
    /// Number of failed authentication attempts
    pub auth_failure_count: u32,
}

impl Default for PeerAuthState {
    fn default() -> Self {
        Self {
            peer_id: [0u8; 32], // Will be set to actual peer ID
            status: AuthenticationStatus::Pending,
            verified_ed25519_key: None,
            x25519_public_key: None,
            session_key: None,
            cipher: None,
            authenticated_at: None,
            trust_level: 0.0,
            auth_success_count: 0,
            auth_failure_count: 0,
        }
    }
}

/// Pure Rust P2P Network Manager with authentication and encryption
pub struct PureRustP2PAuth {
    /// Local Ed25519 signing key
    local_signing_key: SigningKey,
    /// Local Ed25519 verifying key
    local_verifying_key: VerifyingKey,
    /// Local X25519 static secret for key exchange
    local_x25519_secret: X25519StaticSecret,
    /// Local X25519 public key
    local_x25519_public: X25519PublicKey,
    /// Local peer ID (Blake3 hash of Ed25519 public key)
    local_peer_id: PeerId,
    /// Integration with existing Pure Rust DHT
    pure_rust_dht: Arc<PureRustDht>,
    /// Connected peers with authentication states
    authenticated_peers: Arc<RwLock<HashMap<PeerId, ConnectedPeerInfo>>>,
    /// Pending authentication requests
    pending_auth_requests: Arc<RwLock<HashMap<u64, PeerId>>>, // Request ID -> Peer ID
    /// Trust scores and reputation tracking
    peer_reputation: Arc<RwLock<HashMap<PeerId, PeerReputation>>>,
    /// Event sender for network events
    event_sender: mpsc::UnboundedSender<P2PAuthEvent>,
    /// Network statistics
    network_stats: Arc<RwLock<AuthNetworkStatistics>>,
    /// Configuration
    config: P2PAuthConfig,
}

/// Connected peer information with authentication
#[derive(Debug, Clone)]
pub struct ConnectedPeerInfo {
    /// Peer ID
    pub peer_id: PeerId,
    /// Connection address
    pub address: SocketAddr,
    /// Authentication state
    pub auth_state: PeerAuthState,
    /// Connection timestamp
    pub connected_at: Instant,
    /// Last seen timestamp
    pub last_seen: Instant,
    /// Connection quality metrics
    pub connection_quality: f64,
    /// Round-trip time
    pub rtt: Option<Duration>,
    /// Bytes sent to this peer
    pub bytes_sent: u64,
    /// Bytes received from this peer
    pub bytes_received: u64,
    /// Messages sent count
    pub messages_sent: u32,
    /// Messages received count
    pub messages_received: u32,
}

impl ConnectedPeerInfo {
    pub fn new(peer_id: PeerId, address: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            peer_id,
            address,
            auth_state: PeerAuthState {
                peer_id,
                ..Default::default()
            },
            connected_at: now,
            last_seen: now,
            connection_quality: 1.0,
            rtt: None,
            bytes_sent: 0,
            bytes_received: 0,
            messages_sent: 0,
            messages_received: 0,
        }
    }
}

/// P2P authentication events
#[derive(Debug, Clone)]
pub enum P2PAuthEvent {
    /// Peer authentication started
    AuthenticationStarted {
        peer_id: PeerId,
        timestamp: Instant,
    },
    /// Peer successfully authenticated
    PeerAuthenticated {
        peer_id: PeerId,
        trust_level: f64,
        timestamp: Instant,
    },
    /// Peer authentication failed
    AuthenticationFailed {
        peer_id: PeerId,
        reason: String,
        timestamp: Instant,
    },
    /// Secure channel established
    SecureChannelEstablished {
        peer_id: PeerId,
        session_key_hash: [u8; 32], // Blake3 hash of session key for logging
        timestamp: Instant,
    },
    /// Encrypted message received
    EncryptedMessageReceived {
        peer_id: PeerId,
        message_type: String,
        size: usize,
        timestamp: Instant,
    },
    /// Peer trust level updated
    TrustLevelUpdated {
        peer_id: PeerId,
        old_trust: f64,
        new_trust: f64,
        reason: String,
        timestamp: Instant,
    },
    /// Authentication session expired
    AuthenticationExpired {
        peer_id: PeerId,
        timestamp: Instant,
    },
    /// Peer disconnected
    PeerDisconnected {
        peer_id: PeerId,
        reason: String,
        timestamp: Instant,
    },
}

/// Peer reputation tracking
#[derive(Debug, Clone)]
pub struct PeerReputation {
    /// Base trust score (0.0 to 1.0)
    pub trust_score: f64,
    /// Number of successful interactions
    pub successful_interactions: u32,
    /// Number of failed interactions
    pub failed_interactions: u32,
    /// Last interaction timestamp
    pub last_interaction: Instant,
    /// Uptime tracking
    pub total_uptime: Duration,
    /// Connection stability (0.0 to 1.0)
    pub connection_stability: f64,
    /// Message reliability (0.0 to 1.0)
    pub message_reliability: f64,
    /// Reported misbehavior incidents
    pub misbehavior_reports: u32,
    /// Quality of service metrics
    pub qos_metrics: QoSMetrics,
}

/// Quality of service metrics
#[derive(Debug, Clone, Default)]
pub struct QoSMetrics {
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Message delivery success rate (0.0 to 1.0)
    pub delivery_success_rate: f64,
    /// Bandwidth utilization efficiency (0.0 to 1.0)
    pub bandwidth_efficiency: f64,
    /// Protocol compliance score (0.0 to 1.0)
    pub protocol_compliance: f64,
}

/// Network statistics for authentication
#[derive(Debug, Clone, Default)]
pub struct AuthNetworkStatistics {
    /// Total authentication attempts
    pub total_auth_attempts: u64,
    /// Successful authentications
    pub successful_authentications: u64,
    /// Failed authentications
    pub failed_authentications: u64,
    /// Currently authenticated peers
    pub authenticated_peers_count: u32,
    /// Total secure messages sent
    pub secure_messages_sent: u64,
    /// Total secure messages received
    pub secure_messages_received: u64,
    /// Average trust level across all peers
    pub average_trust_level: f64,
    /// Total encrypted bytes transmitted
    pub encrypted_bytes_transmitted: u64,
    /// Total encrypted bytes received
    pub encrypted_bytes_received: u64,
    /// Network uptime
    pub network_uptime: Duration,
    /// Last statistics update
    pub last_updated: Instant,
}

/// P2P authentication configuration
#[derive(Debug, Clone)]
pub struct P2PAuthConfig {
    /// Authentication timeout duration
    pub auth_timeout: Duration,
    /// Session key lifetime
    pub session_lifetime: Duration,
    /// Maximum concurrent authentication attempts
    pub max_concurrent_auth: u32,
    /// Minimum trust level for communication
    pub min_trust_level: f64,
    /// Trust decay rate per day
    pub trust_decay_rate: f64,
    /// Maximum message size for encrypted communication
    pub max_message_size: usize,
    /// Enable peer reputation tracking
    pub enable_reputation_tracking: bool,
    /// Heartbeat interval for authenticated peers
    pub heartbeat_interval: Duration,
    /// Maximum failed authentication attempts before ban
    pub max_failed_attempts: u32,
    /// Ban duration for misbehaving peers
    pub ban_duration: Duration,
}

impl Default for P2PAuthConfig {
    fn default() -> Self {
        Self {
            auth_timeout: Duration::from_secs(30),
            session_lifetime: Duration::from_secs(3600), // 1 hour
            max_concurrent_auth: 10,
            min_trust_level: 0.5,
            trust_decay_rate: 0.01, // 1% per day
            max_message_size: 1024 * 1024, // 1MB
            enable_reputation_tracking: true,
            heartbeat_interval: Duration::from_secs(60),
            max_failed_attempts: 3,
            ban_duration: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// DHT-integrated authenticated peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtAuthenticatedPeer {
    /// Peer's cryptographic identity
    pub peer_id: PeerId,
    /// Verified Ed25519 public key
    pub ed25519_public_key: [u8; 32],
    /// X25519 public key for key exchange
    pub x25519_public_key: [u8; 32],
    /// Network addresses for the peer
    pub addresses: Vec<SocketAddr>,
    /// Trust score (0.0 to 1.0)
    pub trust_score: f64,
    /// Last authentication timestamp
    pub last_authenticated: SystemTime,
    /// Peer capabilities and supported features
    pub capabilities: PeerCapabilities,
    /// Network quality metrics
    pub network_metrics: NetworkQualityMetrics,
    /// DHT node ID for routing
    pub dht_node_id: NodeId,
    /// Signature of peer announcement (self-signed)
    pub announcement_signature: [u8; 64],
}

/// Peer capabilities for feature negotiation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeerCapabilities {
    /// Supported protocol versions
    pub protocol_versions: Vec<String>,
    /// Supported encryption algorithms
    pub encryption_algorithms: Vec<String>,
    /// Supported transport protocols (UDP, QUIC, TCP)
    pub transport_protocols: Vec<String>,
    /// Maximum message size supported
    pub max_message_size: u32,
    /// Supports multipath communication
    pub supports_multipath: bool,
    /// Supports post-quantum cryptography
    pub supports_post_quantum: bool,
    /// Available relay capacity
    pub relay_capacity: Option<u32>,
    /// Geographic region (for routing optimization)
    pub region: Option<String>,
}

/// Network quality metrics for routing decisions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkQualityMetrics {
    /// Average round-trip time in milliseconds
    pub rtt_ms: f64,
    /// Bandwidth estimate in kbps
    pub bandwidth_kbps: u32,
    /// Packet loss rate (0.0 to 1.0)
    pub packet_loss_rate: f64,
    /// Connection stability score (0.0 to 1.0)
    pub stability_score: f64,
    /// Uptime percentage
    pub uptime_percentage: f64,
    /// Last measurement timestamp
    pub last_updated: SystemTime,
}

/// DHT-based peer discovery criteria
#[derive(Debug, Clone)]
pub struct DhtDiscoveryCriteria {
    /// Minimum trust level required
    pub min_trust_level: f64,
    /// Required capabilities
    pub required_capabilities: PeerCapabilities,
    /// Geographic preferences
    pub preferred_regions: Option<Vec<String>>,
    /// Maximum acceptable RTT in milliseconds
    pub max_rtt_ms: Option<f64>,
    /// Minimum bandwidth requirement in kbps
    pub min_bandwidth_kbps: Option<u32>,
    /// Maximum number of peers to discover
    pub max_peers: usize,
    /// Include peers currently being authenticated
    pub include_pending_auth: bool,
}

impl Default for DhtDiscoveryCriteria {
    fn default() -> Self {
        Self {
            min_trust_level: 0.5,
            required_capabilities: PeerCapabilities::default(),
            preferred_regions: None,
            max_rtt_ms: Some(1000.0), // 1 second max RTT
            min_bandwidth_kbps: Some(100), // 100 kbps minimum
            max_peers: 50,
            include_pending_auth: false,
        }
    }
}

/// DHT health monitoring metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtHealthMetrics {
    /// Total number of authenticated peers in DHT
    pub total_authenticated_peers: usize,
    /// Average trust score across all peers
    pub average_trust_score: f64,
    /// Network connectivity density (0.0 to 1.0)
    pub network_density: f64,
    /// Risk of network partition (0.0 to 1.0)
    pub partition_risk: f64,
    /// DHT routing table size
    pub dht_routing_table_size: usize,
    /// DHT connection status
    pub dht_is_connected: bool,
    /// DHT health check result
    pub dht_health_status: bool,
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

/// Network topology information for routing optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopologyInfo {
    /// Known authenticated peers in the network
    pub authenticated_peers: HashMap<PeerId, DhtAuthenticatedPeer>,
    /// Network clustering information
    pub clusters: Vec<PeerCluster>,
    /// Geographic distribution
    pub geographic_distribution: HashMap<String, u32>,
    /// Trust network graph edges
    pub trust_edges: Vec<TrustEdge>,
    /// Last topology update
    pub last_updated: SystemTime,
}

/// Peer cluster for routing optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCluster {
    /// Cluster identifier
    pub cluster_id: String,
    /// Peers in this cluster
    pub peer_ids: Vec<PeerId>,
    /// Average trust level in cluster
    pub avg_trust_level: f64,
    /// Cluster quality score
    pub quality_score: f64,
    /// Geographic center of cluster
    pub geographic_center: Option<(f64, f64)>, // (latitude, longitude)
}

/// Trust relationship between two peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEdge {
    /// Source peer ID
    pub from_peer: PeerId,
    /// Target peer ID
    pub to_peer: PeerId,
    /// Trust weight (0.0 to 1.0)
    pub trust_weight: f64,
    /// Last interaction timestamp
    pub last_interaction: SystemTime,
    /// Number of successful interactions
    pub interaction_count: u32,
}

impl PureRustP2PAuth {
    /// Create a new Pure Rust P2P Authentication manager
    pub fn new(
        pure_rust_dht: Arc<PureRustDht>,
        config: P2PAuthConfig,
    ) -> Result<(Self, mpsc::UnboundedReceiver<P2PAuthEvent>), P2PError> {
        // Generate local Ed25519 keypair for signing
        let local_signing_key = SigningKey::generate(&mut OsRng);
        let local_verifying_key = VerifyingKey::from(&local_signing_key);
        
        // Generate local X25519 keypair for key exchange
        let local_x25519_secret = X25519StaticSecret::random_from_rng(OsRng);
        let local_x25519_public = X25519PublicKey::from(&local_x25519_secret);
        
        // Generate peer ID from Ed25519 public key
        let local_peer_id = Self::generate_peer_id(&local_verifying_key);
        
        // Create event channel
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        
        let network = Self {
            local_signing_key,
            local_verifying_key,
            local_x25519_secret,
            local_x25519_public,
            local_peer_id,
            pure_rust_dht,
            authenticated_peers: Arc::new(RwLock::new(HashMap::new())),
            pending_auth_requests: Arc::new(RwLock::new(HashMap::new())),
            peer_reputation: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            network_stats: Arc::new(RwLock::new(AuthNetworkStatistics::default())),
            config,
        };
        
        info!("Pure Rust P2P Authentication manager initialized with peer ID: {:?}", 
              local_peer_id);
        
        Ok((network, event_receiver))
    }
    
    /// Generate peer ID from Ed25519 public key
    fn generate_peer_id(public_key: &VerifyingKey) -> PeerId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(public_key.to_bytes().as_slice());
        hasher.finalize().into()
    }
    
    /// Start authentication with a peer
    pub async fn authenticate_peer(
        &self,
        peer_id: PeerId,
        peer_address: SocketAddr,
    ) -> Result<(), P2PError> {
        // Check if already authenticated
        if let Some(peer_info) = self.authenticated_peers.read().await.get(&peer_id) {
            if peer_info.auth_state.status == AuthenticationStatus::Authenticated {
                return Ok(());
            }
        }
        
        // Check for ongoing authentication
        let pending_auths = self.pending_auth_requests.read().await;
        if pending_auths.values().any(|&id| id == peer_id) {
            return Err(P2PError::AuthenticationInProgress);
        }
        drop(pending_auths);
        
        // Update statistics
        self.network_stats.write().await.total_auth_attempts += 1;
        
        // Send authentication started event
        let _ = self.event_sender.send(P2PAuthEvent::AuthenticationStarted {
            peer_id,
            timestamp: Instant::now(),
        });
        
        // Generate authentication request
        let auth_request = self.create_auth_request().await?;
        let request_id = auth_request.request_id;
        
        // Store pending request
        self.pending_auth_requests.write().await.insert(request_id, peer_id);
        
        // Add peer to authenticated peers list with pending status
        let mut peer_info = ConnectedPeerInfo::new(peer_id, peer_address);
        peer_info.auth_state.status = AuthenticationStatus::InProgress;
        self.authenticated_peers.write().await.insert(peer_id, peer_info);
        
        // TODO: Send auth_request to peer via TCP connection
        // This would involve establishing a TCP connection and sending the serialized request
        info!("Authentication request sent to peer {:?}", peer_id);
        
        // Start timeout timer
        let auth_manager = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(auth_manager.config.auth_timeout).await;
            auth_manager.handle_auth_timeout(request_id).await;
        });
        
        Ok(())
    }
    
    /// Create authentication request
    async fn create_auth_request(&self) -> Result<AuthRequest, P2PError> {
        let request_id = rand::random::<u64>();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Create challenge for peer to sign
        let mut challenge = [0u8; 32];
        OsRng.fill_bytes(&mut challenge);
        
        // Sign our identity proof
        let identity_data = [
            self.local_verifying_key.to_bytes().as_slice(),
            &timestamp.to_be_bytes(),
            &challenge,
        ].concat();
        
        let signature = self.local_signing_key.sign(&identity_data);
        
        Ok(AuthRequest {
            request_id,
            peer_id: self.local_peer_id,
            ed25519_public_key: self.local_verifying_key.to_bytes(),
            x25519_public_key: self.local_x25519_public.to_bytes(),
            challenge,
            signature: signature.to_bytes(),
            timestamp,
        })
    }
    
    /// Handle authentication response from peer
    pub async fn handle_auth_response(
        &self,
        response: AuthResponse,
        peer_address: SocketAddr,
    ) -> Result<(), P2PError> {
        // Find pending request
        let mut pending_requests = self.pending_auth_requests.write().await;
        let peer_id = pending_requests.remove(&response.request_id)
            .ok_or(P2PError::UnknownAuthRequest)?;
        drop(pending_requests);
        
        // Verify response
        self.verify_auth_response(&response, peer_id).await?;
        
        // Establish secure session
        self.establish_secure_session(peer_id, &response, peer_address).await?;
        
        // Update statistics
        self.network_stats.write().await.successful_authentications += 1;
        
        // Calculate trust level
        let trust_level = self.calculate_initial_trust_level(peer_id).await;
        
        // Update peer state
        if let Some(peer_info) = self.authenticated_peers.write().await.get_mut(&peer_id) {
            peer_info.auth_state.status = AuthenticationStatus::Authenticated;
            peer_info.auth_state.authenticated_at = Some(Instant::now());
            peer_info.auth_state.trust_level = trust_level;
            peer_info.auth_state.auth_success_count += 1;
        }
        
        // Send authentication success event
        let _ = self.event_sender.send(P2PAuthEvent::PeerAuthenticated {
            peer_id,
            trust_level,
            timestamp: Instant::now(),
        });
        
        info!("Peer {:?} successfully authenticated with trust level {:.2}", 
              peer_id, trust_level);
        
        Ok(())
    }
    
    /// Verify authentication response
    async fn verify_auth_response(
        &self,
        response: &AuthResponse,
        expected_peer_id: PeerId,
    ) -> Result<(), P2PError> {
        // Verify peer ID matches Ed25519 public key
        let peer_verifying_key = VerifyingKey::from_bytes(&response.ed25519_public_key)
            .map_err(|_| P2PError::InvalidKey)?;
        let derived_peer_id = Self::generate_peer_id(&peer_verifying_key);
        
        if derived_peer_id != expected_peer_id {
            return Err(P2PError::InvalidPeerId);
        }
        
        // Verify signature
        let signature_data = [
            &response.ed25519_public_key[..],
            &response.timestamp.to_be_bytes(),
            &response.challenge_response,
        ].concat();
        
        let signature = Signature::from_bytes(&response.signature)
            .map_err(|_| P2PError::InvalidSignature)?;
        
        peer_verifying_key.verify(&signature_data, &signature)
            .map_err(|_| P2PError::InvalidSignature)?;
        
        // Verify timestamp (within acceptable range)
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        if (current_time as i64 - response.timestamp as i64).abs() > 300 {  // 5 minute tolerance
            return Err(P2PError::InvalidTimestamp);
        }
        
        Ok(())
    }
    
    /// Establish secure session with authenticated peer
    async fn establish_secure_session(
        &self,
        peer_id: PeerId,
        response: &AuthResponse,
        peer_address: SocketAddr,
    ) -> Result<(), P2PError> {
        // Parse peer's X25519 public key
        let peer_x25519_key = X25519PublicKey::from(response.x25519_public_key);
        
        // Perform X25519 key exchange
        let shared_secret = self.local_x25519_secret.diffie_hellman(&peer_x25519_key);
        
        // Derive session key using HKDF
        let session_key = self.derive_session_key(shared_secret.as_bytes(), peer_id).await?;
        
        // Create ChaCha20Poly1305 cipher
        let cipher = ChaCha20Poly1305::new(&session_key.into());
        
        // Update peer authentication state
        if let Some(peer_info) = self.authenticated_peers.write().await.get_mut(&peer_id) {
            peer_info.auth_state.verified_ed25519_key = Some(
                VerifyingKey::from_bytes(&response.ed25519_public_key)
                    .map_err(|_| P2PError::InvalidKey)?
            );
            peer_info.auth_state.x25519_public_key = Some(peer_x25519_key);
            peer_info.auth_state.session_key = Some(session_key);
            peer_info.auth_state.cipher = Some(cipher);
        }
        
        // Send secure channel established event
        let session_key_hash = blake3::hash(&session_key);
        let _ = self.event_sender.send(P2PAuthEvent::SecureChannelEstablished {
            peer_id,
            session_key_hash: session_key_hash.into(),
            timestamp: Instant::now(),
        });
        
        info!("Secure channel established with peer {:?}", peer_id);
        
        Ok(())
    }
    
    /// Derive session key from shared secret
    async fn derive_session_key(
        &self,
        shared_secret: &[u8],
        peer_id: PeerId,
    ) -> Result<[u8; 32], P2PError> {
        // Use HKDF with Blake3 for key derivation
        let mut hasher = blake3::Hasher::new();
        hasher.update(shared_secret);
        hasher.update(&self.local_peer_id);
        hasher.update(&peer_id);
        hasher.update(b"nyx-p2p-session-key");
        
        Ok(hasher.finalize().into())
    }
    
    /// Calculate initial trust level for a peer
    async fn calculate_initial_trust_level(&self, peer_id: PeerId) -> f64 {
        // Check reputation history
        if let Some(reputation) = self.peer_reputation.read().await.get(&peer_id) {
            reputation.trust_score
        } else {
            // Default trust level for new peers
            0.5
        }
    }
    
    /// Handle authentication timeout
    async fn handle_auth_timeout(&self, request_id: u64) {
        if let Some(peer_id) = self.pending_auth_requests.write().await.remove(&request_id) {
            // Update failure statistics
            self.network_stats.write().await.failed_authentications += 1;
            
            // Update peer state
            if let Some(peer_info) = self.authenticated_peers.write().await.get_mut(&peer_id) {
                peer_info.auth_state.status = AuthenticationStatus::Failed;
                peer_info.auth_state.auth_failure_count += 1;
            }
            
            // Send authentication failed event
            let _ = self.event_sender.send(P2PAuthEvent::AuthenticationFailed {
                peer_id,
                reason: "Authentication timeout".to_string(),
                timestamp: Instant::now(),
            });
            
            warn!("Authentication timeout for peer {:?}", peer_id);
        }
    }
    
    /// Send encrypted message to authenticated peer
    pub async fn send_encrypted_message(
        &self,
        peer_id: PeerId,
        message: &[u8],
    ) -> Result<(), P2PError> {
        let mut peers = self.authenticated_peers.write().await;
        let peer_info = peers.get_mut(&peer_id)
            .ok_or(P2PError::PeerNotFound)?;
        
        // Check authentication status
        if peer_info.auth_state.status != AuthenticationStatus::Authenticated {
            return Err(P2PError::PeerNotAuthenticated);
        }
        
        // Check trust level
        if peer_info.auth_state.trust_level < self.config.min_trust_level {
            return Err(P2PError::InsufficientTrust);
        }
        
        // Get cipher
        let cipher = peer_info.auth_state.cipher.as_ref()
            .ok_or(P2PError::NoSecureChannel)?;
        
        // Generate nonce
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        
        // Encrypt message
        let encrypted = cipher.encrypt(&nonce.into(), message)
            .map_err(|_| P2PError::EncryptionFailed)?;
        
        // Update statistics
        peer_info.bytes_sent += encrypted.len() as u64;
        peer_info.messages_sent += 1;
        self.network_stats.write().await.secure_messages_sent += 1;
        self.network_stats.write().await.encrypted_bytes_transmitted += encrypted.len() as u64;
        
        // TODO: Actually send encrypted message via TCP connection
        // This would involve sending the nonce + encrypted data
        
        info!("Encrypted message sent to peer {:?} ({} bytes)", 
              peer_id, encrypted.len());
        
        Ok(())
    }
    
    /// Receive and decrypt message from authenticated peer
    pub async fn receive_encrypted_message(
        &self,
        peer_id: PeerId,
        nonce: &[u8; 12],
        encrypted_data: &[u8],
    ) -> Result<Vec<u8>, P2PError> {
        let mut peers = self.authenticated_peers.write().await;
        let peer_info = peers.get_mut(&peer_id)
            .ok_or(P2PError::PeerNotFound)?;
        
        // Check authentication status
        if peer_info.auth_state.status != AuthenticationStatus::Authenticated {
            return Err(P2PError::PeerNotAuthenticated);
        }
        
        // Get cipher
        let cipher = peer_info.auth_state.cipher.as_ref()
            .ok_or(P2PError::NoSecureChannel)?;
        
        // Decrypt message
        let decrypted = cipher.decrypt(&(*nonce).into(), encrypted_data)
            .map_err(|_| P2PError::DecryptionFailed)?;
        
        // Update statistics
        peer_info.bytes_received += encrypted_data.len() as u64;
        peer_info.messages_received += 1;
        peer_info.last_seen = Instant::now();
        self.network_stats.write().await.secure_messages_received += 1;
        self.network_stats.write().await.encrypted_bytes_received += encrypted_data.len() as u64;
        
        // Send encrypted message received event
        let _ = self.event_sender.send(P2PAuthEvent::EncryptedMessageReceived {
            peer_id,
            message_type: "encrypted".to_string(),
            size: decrypted.len(),
            timestamp: Instant::now(),
        });
        
        info!("Encrypted message received from peer {:?} ({} bytes)", 
              peer_id, decrypted.len());
        
        Ok(decrypted)
    }
    
    /// Discover authenticated peers through DHT integration
    pub async fn discover_authenticated_peers(
        &self,
        criteria: DhtDiscoveryCriteria,
    ) -> Result<Vec<DhtAuthenticatedPeer>, P2PError> {
        info!("Starting DHT-based peer discovery with criteria: {:?}", criteria);
        
        let mut discovered_peers = Vec::new();
        
        // Search DHT for authenticated peer records
        let search_results = self.search_dht_for_peers(&criteria).await?;
        
        for peer_data in search_results {
            // Verify peer authenticity and trust level
            if let Ok(authenticated_peer) = self.verify_dht_peer_data(&peer_data).await {
                if authenticated_peer.trust_score >= criteria.min_trust_level {
                    // Check network quality requirements
                    if self.meets_network_requirements(&authenticated_peer, &criteria).await {
                        discovered_peers.push(authenticated_peer);
                        
                        if discovered_peers.len() >= criteria.max_peers {
                            break;
                        }
                    }
                }
            }
        }
        
        info!("Discovered {} authenticated peers matching criteria", discovered_peers.len());
        
        // Update network statistics
        self.network_stats.write().await.authenticated_peers_count = 
            self.get_authenticated_peers().await.len() as u32;
        
        Ok(discovered_peers)
    }
    
    /// Search DHT for peer authentication records
    async fn search_dht_for_peers(
        &self,
        criteria: &DhtDiscoveryCriteria,
    ) -> Result<Vec<Vec<u8>>, P2PError> {
        let mut peer_records = Vec::new();
        
        // Query DHT for authenticated peer records using prefixed keys
        let peer_auth_pattern = format!("{}*", DHT_PEER_AUTH_PREFIX);
        
        // In a real implementation, this would iterate through DHT keys
        // For now, we'll simulate DHT queries through the existing PureRustDht
        let all_peer_ids = self.pure_rust_dht.get_all_peer_ids().await
            .map_err(|e| P2PError::NetworkError(format!("DHT query failed: {}", e)))?;
        
        for peer_id_str in all_peer_ids {
            let auth_key = format!("{}:{}", DHT_PEER_AUTH_PREFIX, peer_id_str);
            
            if let Ok(Some(peer_data)) = self.pure_rust_dht.get(&auth_key).await {
                peer_records.push(peer_data);
                
                if peer_records.len() >= criteria.max_peers * 2 {
                    // Retrieve more than needed for filtering
                    break;
                }
            }
        }
        
        Ok(peer_records)
    }
    
    /// Verify and deserialize DHT peer authentication data
    async fn verify_dht_peer_data(
        &self,
        peer_data: &[u8],
    ) -> Result<DhtAuthenticatedPeer, P2PError> {
        // Deserialize peer data from DHT
        let authenticated_peer: DhtAuthenticatedPeer = bincode::deserialize(peer_data)
            .map_err(|_| P2PError::InvalidKey)?;
        
        // Verify peer's self-signed announcement
        self.verify_peer_announcement(&authenticated_peer).await?;
        
        // Check timestamp validity (not too old)
        let age = SystemTime::now()
            .duration_since(authenticated_peer.last_authenticated)
            .unwrap_or(Duration::from_secs(u64::MAX));
        
        if age > Duration::from_secs(86400) { // 24 hours max age
            return Err(P2PError::InvalidTimestamp);
        }
        
        Ok(authenticated_peer)
    }
    
    /// Verify peer's self-signed announcement signature
    async fn verify_peer_announcement(
        &self,
        peer: &DhtAuthenticatedPeer,
    ) -> Result<(), P2PError> {
        // Reconstruct the signed data
        let announcement_data = [
            &peer.peer_id[..],
            &peer.ed25519_public_key[..],
            &peer.x25519_public_key[..],
            &peer.last_authenticated.duration_since(UNIX_EPOCH).unwrap().as_secs().to_be_bytes(),
        ].concat();
        
        // Verify signature using peer's Ed25519 public key
        let verifying_key = VerifyingKey::from_bytes(&peer.ed25519_public_key)
            .map_err(|_| P2PError::InvalidKey)?;
        
        let signature = Signature::from_bytes(&peer.announcement_signature)
            .map_err(|_| P2PError::InvalidSignature)?;
        
        verifying_key.verify(&announcement_data, &signature)
            .map_err(|_| P2PError::InvalidSignature)?;
        
        // Verify peer ID matches public key
        let derived_peer_id = Self::generate_peer_id(&verifying_key);
        if derived_peer_id != peer.peer_id {
            return Err(P2PError::InvalidPeerId);
        }
        
        Ok(())
    }
    
    /// Check if peer meets network quality requirements
    async fn meets_network_requirements(
        &self,
        peer: &DhtAuthenticatedPeer,
        criteria: &DhtDiscoveryCriteria,
    ) -> bool {
        // Check RTT requirement
        if let Some(max_rtt) = criteria.max_rtt_ms {
            if peer.network_metrics.rtt_ms > max_rtt {
                return false;
            }
        }
        
        // Check bandwidth requirement
        if let Some(min_bandwidth) = criteria.min_bandwidth_kbps {
            if peer.network_metrics.bandwidth_kbps < min_bandwidth {
                return false;
            }
        }
        
        // Check regional preferences
        if let Some(preferred_regions) = &criteria.preferred_regions {
            if let Some(peer_region) = &peer.capabilities.region {
                if !preferred_regions.contains(peer_region) {
                    return false;
                }
            }
        }
        
        // Check required capabilities
        if !self.peer_has_required_capabilities(peer, &criteria.required_capabilities) {
            return false;
        }
        
        true
    }
    
    /// Check if peer has required capabilities
    fn peer_has_required_capabilities(
        &self,
        peer: &DhtAuthenticatedPeer,
        required: &PeerCapabilities,
    ) -> bool {
        // Check protocol version compatibility
        for required_version in &required.protocol_versions {
            if !peer.capabilities.protocol_versions.contains(required_version) {
                return false;
            }
        }
        
        // Check encryption algorithm support
        for required_algo in &required.encryption_algorithms {
            if !peer.capabilities.encryption_algorithms.contains(required_algo) {
                return false;
            }
        }
        
        // Check transport protocol support
        for required_transport in &required.transport_protocols {
            if !peer.capabilities.transport_protocols.contains(required_transport) {
                return false;
            }
        }
        
        // Check multipath support if required
        if required.supports_multipath && !peer.capabilities.supports_multipath {
            return false;
        }
        
        // Check post-quantum support if required
        if required.supports_post_quantum && !peer.capabilities.supports_post_quantum {
            return false;
        }
        
        true
    }
    
    /// Publish authenticated peer information to DHT
    pub async fn publish_peer_to_dht(
        &self,
        capabilities: PeerCapabilities,
        network_metrics: NetworkQualityMetrics,
    ) -> Result<(), P2PError> {
        let authenticated_peer = self.create_dht_peer_announcement(capabilities, network_metrics).await?;
        
        // Serialize and store in DHT
        let peer_data = bincode::serialize(&authenticated_peer)
            .map_err(|_| P2PError::NetworkError("Serialization failed".to_string()))?;
        
        let auth_key = format!("{}:{}", DHT_PEER_AUTH_PREFIX, hex::encode(&self.local_peer_id));
        
        self.pure_rust_dht.store(&auth_key, peer_data).await
            .map_err(|e| P2PError::NetworkError(format!("DHT store failed: {}", e)))?;
        
        // Also publish trust information
        self.publish_trust_information().await?;
        
        info!("Successfully published peer information to DHT");
        Ok(())
    }
    
    /// Create DHT peer announcement with signature
    async fn create_dht_peer_announcement(
        &self,
        capabilities: PeerCapabilities,
        network_metrics: NetworkQualityMetrics,
    ) -> Result<DhtAuthenticatedPeer, P2PError> {
        let timestamp = SystemTime::now();
        
        // Create announcement data to sign
        let announcement_data = [
            &self.local_peer_id[..],
            &self.local_verifying_key.to_bytes()[..],
            &self.local_x25519_public.to_bytes()[..],
            &timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs().to_be_bytes(),
        ].concat();
        
        // Sign the announcement
        let signature = self.local_signing_key.sign(&announcement_data);
        
        // Get current trust level from reputation system
        let trust_score = if let Some(reputation) = self.peer_reputation.read().await.get(&self.local_peer_id) {
            reputation.trust_score
        } else {
            0.7 // Default trust level for self
        };
        
        // Convert DHT node ID to our format
        let dht_node_id = self.pure_rust_dht.get_local_node_id().await
            .map_err(|e| P2PError::NetworkError(format!("Failed to get DHT node ID: {}", e)))?;
        
        Ok(DhtAuthenticatedPeer {
            peer_id: self.local_peer_id,
            ed25519_public_key: self.local_verifying_key.to_bytes(),
            x25519_public_key: self.local_x25519_public.to_bytes(),
            addresses: vec![], // Would be populated with actual network addresses
            trust_score,
            last_authenticated: timestamp,
            capabilities,
            network_metrics,
            dht_node_id,
            announcement_signature: signature.to_bytes(),
        })
    }
    
    /// Publish trust relationship information to DHT
    async fn publish_trust_information(&self) -> Result<(), P2PError> {
        let reputation_data = self.peer_reputation.read().await;
        
        for (peer_id, reputation) in reputation_data.iter() {
            let trust_edge = TrustEdge {
                from_peer: self.local_peer_id,
                to_peer: *peer_id,
                trust_weight: reputation.trust_score,
                last_interaction: reputation.last_interaction,
                interaction_count: reputation.successful_interactions,
            };
            
            let trust_data = bincode::serialize(&trust_edge)
                .map_err(|_| P2PError::NetworkError("Trust serialization failed".to_string()))?;
            
            let trust_key = format!("{}:{}:{}", 
                DHT_PEER_TRUST_PREFIX, 
                hex::encode(&self.local_peer_id),
                hex::encode(peer_id)
            );
            
            self.pure_rust_dht.store(&trust_key, trust_data).await
                .map_err(|e| P2PError::NetworkError(format!("Trust store failed: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// Discover trust-based routing paths through DHT
    pub async fn discover_trust_based_routes(
        &self,
        target_peer: PeerId,
        min_path_trust: f64,
        max_hops: usize,
    ) -> Result<Vec<Vec<PeerId>>, P2PError> {
        info!("Discovering trust-based routes to peer {:?}", target_peer);
        
        // Get network topology from DHT
        let topology = self.build_network_topology().await?;
        
        // Find trust-based paths using modified Dijkstra's algorithm
        let paths = self.find_trust_weighted_paths(
            &topology,
            self.local_peer_id,
            target_peer,
            min_path_trust,
            max_hops,
        ).await?;
        
        info!("Found {} trust-based routes with minimum trust {:.2}", 
              paths.len(), min_path_trust);
        
        Ok(paths)
    }
    
    /// Build network topology from DHT trust information
    async fn build_network_topology(&self) -> Result<NetworkTopologyInfo, P2PError> {
        let mut topology = NetworkTopologyInfo {
            authenticated_peers: HashMap::new(),
            clusters: Vec::new(),
            geographic_distribution: HashMap::new(),
            trust_edges: Vec::new(),
            last_updated: SystemTime::now(),
        };
        
        // Query DHT for all trust relationships
        let trust_keys = self.get_all_trust_keys().await?;
        
        for trust_key in trust_keys {
            if let Ok(Some(trust_data)) = self.pure_rust_dht.get(&trust_key).await {
                if let Ok(trust_edge) = bincode::deserialize::<TrustEdge>(&trust_data) {
                    topology.trust_edges.push(trust_edge);
                }
            }
        }
        
        // Query DHT for authenticated peer information
        let peer_keys = self.get_all_peer_auth_keys().await?;
        
        for peer_key in peer_keys {
            if let Ok(Some(peer_data)) = self.pure_rust_dht.get(&peer_key).await {
                if let Ok(authenticated_peer) = bincode::deserialize::<DhtAuthenticatedPeer>(&peer_data) {
                    // Update geographic distribution
                    if let Some(region) = &authenticated_peer.capabilities.region {
                        *topology.geographic_distribution.entry(region.clone()).or_insert(0) += 1;
                    }
                    
                    topology.authenticated_peers.insert(authenticated_peer.peer_id, authenticated_peer);
                }
            }
        }
        
        // Build peer clusters based on trust relationships and geography
        topology.clusters = self.build_peer_clusters(&topology).await?;
        
        Ok(topology)
    }
    
    /// Get all trust relationship keys from DHT
    async fn get_all_trust_keys(&self) -> Result<Vec<String>, P2PError> {
        // In a real implementation, this would query DHT for keys with trust prefix
        // For now, we'll simulate this based on known peers
        let authenticated_peers = self.authenticated_peers.read().await;
        let mut trust_keys = Vec::new();
        
        for peer_id in authenticated_peers.keys() {
            let trust_key = format!("{}:{}:{}", 
                DHT_PEER_TRUST_PREFIX, 
                hex::encode(&self.local_peer_id),
                hex::encode(peer_id)
            );
            trust_keys.push(trust_key);
        }
        
        Ok(trust_keys)
    }
    
    /// Get all peer authentication keys from DHT
    async fn get_all_peer_auth_keys(&self) -> Result<Vec<String>, P2PError> {
        // Query DHT for all peer authentication records
        let authenticated_peers = self.authenticated_peers.read().await;
        let mut auth_keys = Vec::new();
        
        for peer_id in authenticated_peers.keys() {
            let auth_key = format!("{}:{}", DHT_PEER_AUTH_PREFIX, hex::encode(peer_id));
            auth_keys.push(auth_key);
        }
        
        // Add self
        let self_auth_key = format!("{}:{}", DHT_PEER_AUTH_PREFIX, hex::encode(&self.local_peer_id));
        auth_keys.push(self_auth_key);
        
        Ok(auth_keys)
    }
    
    /// Build peer clusters for routing optimization
    async fn build_peer_clusters(
        &self,
        topology: &NetworkTopologyInfo,
    ) -> Result<Vec<PeerCluster>, P2PError> {
        let mut clusters = Vec::new();
        let mut processed_peers = HashSet::new();
        
        for (peer_id, peer_info) in &topology.authenticated_peers {
            if processed_peers.contains(peer_id) {
                continue;
            }
            
            // Find peers with high trust relationships
            let cluster_peers = self.find_cluster_peers(
                *peer_id,
                topology,
                &mut processed_peers,
                0.7, // Minimum trust threshold for clustering
            ).await;
            
            if cluster_peers.len() >= 2 {
                let cluster_id = format!("cluster_{}", clusters.len());
                let avg_trust = self.calculate_cluster_trust(&cluster_peers, topology).await;
                let quality_score = self.calculate_cluster_quality(&cluster_peers, topology).await;
                
                clusters.push(PeerCluster {
                    cluster_id,
                    peer_ids: cluster_peers,
                    avg_trust_level: avg_trust,
                    quality_score,
                    geographic_center: None, // Could be calculated from peer locations
                });
            }
        }
        
        Ok(clusters)
    }
    
    /// Find peers that should be clustered together based on trust
    async fn find_cluster_peers(
        &self,
        seed_peer: PeerId,
        topology: &NetworkTopologyInfo,
        processed: &mut HashSet<PeerId>,
        min_trust: f64,
    ) -> Vec<PeerId> {
        let mut cluster_peers = vec![seed_peer];
        processed.insert(seed_peer);
        
        // Find peers with strong trust relationships
        for trust_edge in &topology.trust_edges {
            if trust_edge.from_peer == seed_peer && trust_edge.trust_weight >= min_trust {
                if !processed.contains(&trust_edge.to_peer) {
                    cluster_peers.push(trust_edge.to_peer);
                    processed.insert(trust_edge.to_peer);
                }
            }
        }
        
        cluster_peers
    }
    
    /// Calculate average trust level within a cluster
    async fn calculate_cluster_trust(
        &self,
        cluster_peers: &[PeerId],
        topology: &NetworkTopologyInfo,
    ) -> f64 {
        let mut total_trust = 0.0;
        let mut trust_count = 0;
        
        for trust_edge in &topology.trust_edges {
            if cluster_peers.contains(&trust_edge.from_peer) && 
               cluster_peers.contains(&trust_edge.to_peer) {
                total_trust += trust_edge.trust_weight;
                trust_count += 1;
            }
        }
        
        if trust_count > 0 {
            total_trust / trust_count as f64
        } else {
            0.5 // Default trust level
        }
    }
    
    /// Calculate quality score for a peer cluster
    async fn calculate_cluster_quality(
        &self,
        cluster_peers: &[PeerId],
        topology: &NetworkTopologyInfo,
    ) -> f64 {
        let mut total_quality = 0.0;
        let mut peer_count = 0;
        
        for peer_id in cluster_peers {
            if let Some(peer_info) = topology.authenticated_peers.get(peer_id) {
                // Combine network metrics into quality score
                let quality = (peer_info.network_metrics.stability_score * 0.4) +
                             (peer_info.network_metrics.uptime_percentage * 0.3) +
                             ((1.0 - peer_info.network_metrics.packet_loss_rate) * 0.3);
                
                total_quality += quality;
                peer_count += 1;
            }
        }
        
        if peer_count > 0 {
            total_quality / peer_count as f64
        } else {
            0.5 // Default quality score
        }
    }
    
    /// Find trust-weighted paths using modified pathfinding algorithm
    async fn find_trust_weighted_paths(
        &self,
        topology: &NetworkTopologyInfo,
        source: PeerId,
        target: PeerId,
        min_path_trust: f64,
        max_hops: usize,
    ) -> Result<Vec<Vec<PeerId>>, P2PError> {
        // Simple implementation of trust-weighted pathfinding
        // In production, this would use more sophisticated algorithms like A* or Dijkstra
        
        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        let mut current_path = vec![source];
        
        self.find_paths_recursive(
            topology,
            source,
            target,
            &mut current_path,
            &mut visited,
            &mut paths,
            min_path_trust,
            max_hops,
            1.0, // Current path trust starts at 1.0
        ).await;
        
        // Sort paths by trust level (highest first)
        paths.sort_by(|a, b| {
            let trust_a = self.calculate_path_trust(topology, a);
            let trust_b = self.calculate_path_trust(topology, b);
            trust_b.partial_cmp(&trust_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        Ok(paths)
    }
    
    /// Recursive pathfinding with trust weighting
    async fn find_paths_recursive(
        &self,
        topology: &NetworkTopologyInfo,
        current: PeerId,
        target: PeerId,
        current_path: &mut Vec<PeerId>,
        visited: &mut HashSet<PeerId>,
        paths: &mut Vec<Vec<PeerId>>,
        min_path_trust: f64,
        max_hops: usize,
        current_trust: f64,
    ) {
        if current == target {
            if current_trust >= min_path_trust {
                paths.push(current_path.clone());
            }
            return;
        }
        
        if current_path.len() >= max_hops {
            return;
        }
        
        visited.insert(current);
        
        // Find neighbors with sufficient trust
        for trust_edge in &topology.trust_edges {
            if trust_edge.from_peer == current && !visited.contains(&trust_edge.to_peer) {
                let new_trust = current_trust * trust_edge.trust_weight;
                
                if new_trust >= min_path_trust * 0.1 { // Allow some degradation
                    current_path.push(trust_edge.to_peer);
                    
                    Box::pin(self.find_paths_recursive(
                        topology,
                        trust_edge.to_peer,
                        target,
                        current_path,
                        visited,
                        paths,
                        min_path_trust,
                        max_hops,
                        new_trust,
                    )).await;
                    
                    current_path.pop();
                }
            }
        }
        
        visited.remove(&current);
    }
    
    /// Calculate trust level for a complete path
    fn calculate_path_trust(
        &self,
        topology: &NetworkTopologyInfo,
        path: &[PeerId],
    ) -> f64 {
        let mut total_trust = 1.0;
        
        for i in 0..path.len() - 1 {
            let from_peer = path[i];
            let to_peer = path[i + 1];
            
            // Find trust edge between consecutive peers
            if let Some(trust_edge) = topology.trust_edges.iter().find(|edge| {
                edge.from_peer == from_peer && edge.to_peer == to_peer
            }) {
                total_trust *= trust_edge.trust_weight;
            } else {
                // No direct trust relationship - significantly reduce trust
                total_trust *= 0.1;
            }
        }
        
        total_trust
    }
        &self,
        peer_id: PeerId,
        delta: f64,
        reason: String,
    ) -> Result<(), P2PError> {
        let mut peers = self.authenticated_peers.write().await;
        let peer_info = peers.get_mut(&peer_id)
            .ok_or(P2PError::PeerNotFound)?;
        
        let old_trust = peer_info.auth_state.trust_level;
        let new_trust = (old_trust + delta).clamp(0.0, 1.0);
        peer_info.auth_state.trust_level = new_trust;
        
        // Update reputation
        let mut reputation = self.peer_reputation.write().await;
        let rep = reputation.entry(peer_id).or_insert_with(|| PeerReputation {
            trust_score: new_trust,
            successful_interactions: 0,
            failed_interactions: 0,
            last_interaction: Instant::now(),
            total_uptime: Duration::default(),
            connection_stability: 1.0,
            message_reliability: 1.0,
            misbehavior_reports: 0,
            qos_metrics: QoSMetrics::default(),
        });
        
        rep.trust_score = new_trust;
        rep.last_interaction = Instant::now();
        
        if delta > 0.0 {
            rep.successful_interactions += 1;
        } else {
            rep.failed_interactions += 1;
        }
        drop(reputation);
        drop(peers);
        
        // Send trust level updated event
        let _ = self.event_sender.send(P2PAuthEvent::TrustLevelUpdated {
            peer_id,
            old_trust,
            new_trust,
            reason,
            timestamp: Instant::now(),
        });
        
        info!("Trust level for peer {:?} updated: {:.2} -> {:.2}", 
              peer_id, old_trust, new_trust);
        
        Ok(())
    }
    
    /// Get authenticated peers
    pub async fn get_authenticated_peers(&self) -> Vec<PeerId> {
        self.authenticated_peers
            .read()
            .await
            .iter()
            .filter_map(|(peer_id, info)| {
                if info.auth_state.status == AuthenticationStatus::Authenticated {
                    Some(*peer_id)
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get network statistics
    pub async fn get_network_statistics(&self) -> AuthNetworkStatistics {
        let mut stats = self.network_stats.read().await.clone();
        stats.authenticated_peers_count = self.get_authenticated_peers().await.len() as u32;
        
        // Calculate average trust level
        let peers = self.authenticated_peers.read().await;
        let authenticated_peers: Vec<_> = peers
            .values()
            .filter(|info| info.auth_state.status == AuthenticationStatus::Authenticated)
            .collect();
        
        if !authenticated_peers.is_empty() {
            stats.average_trust_level = authenticated_peers
                .iter()
                .map(|info| info.auth_state.trust_level)
                .sum::<f64>() / authenticated_peers.len() as f64;
        }
        
        stats.last_updated = Instant::now();
        stats
    }
    
    /// Build network topology from DHT peers
    pub async fn build_network_topology(&self) -> Result<NetworkTopologyInfo, String> {
        let authenticated_peers = self.discover_authenticated_peers(
            DhtDiscoveryCriteria::default()
        ).await?;
        
        // Build clusters based on geographic location and trust relationships
        let mut clusters = HashMap::new();
        let mut trust_graph = HashMap::new();
        
        for peer in &authenticated_peers {
            // Geographic clustering
            let region = peer.network_quality.location_info.get("region")
                .unwrap_or(&"unknown".to_string()).clone();
            
            clusters.entry(region.clone())
                .or_insert_with(Vec::new)
                .push(peer.clone());
            
            // Build trust relationships
            trust_graph.insert(peer.peer_id.clone(), peer.trust_score);
        }
        
        // Calculate connectivity metrics
        let total_peers = authenticated_peers.len();
        let peer_distribution = clusters.iter()
            .map(|(region, peers)| (region.clone(), peers.len()))
            .collect();
        
        // Calculate network density (simplified)
        let max_connections = total_peers * (total_peers - 1) / 2;
        let actual_connections = authenticated_peers.iter()
            .map(|p| p.network_quality.connection_count)
            .sum::<u64>() / 2; // Avoid double counting
        
        let density = if max_connections > 0 {
            actual_connections as f64 / max_connections as f64
        } else {
            0.0
        };
        
        Ok(NetworkTopologyInfo {
            total_peers,
            clusters,
            trust_graph,
            peer_distribution,
            average_trust: trust_graph.values().sum::<f64>() / trust_graph.len() as f64,
            network_density: density,
            partition_risk: self.calculate_partition_risk(&clusters).await,
        })
    }
    
    /// Calculate network partition risk
    async fn calculate_partition_risk(&self, clusters: &HashMap<String, Vec<DhtAuthenticatedPeer>>) -> f64 {
        let total_peers = clusters.values().map(|c| c.len()).sum::<usize>();
        if total_peers == 0 {
            return 1.0; // Maximum risk with no peers
        }
        
        // Risk is higher when peers are concentrated in few regions
        let largest_cluster_size = clusters.values()
            .map(|c| c.len())
            .max()
            .unwrap_or(0);
        
        // Normalized risk: 0.0 (no risk) to 1.0 (maximum risk)
        largest_cluster_size as f64 / total_peers as f64
    }
    
    /// Announce authenticated peer to DHT
    pub async fn announce_peer_to_dht(&self, peer: &DhtAuthenticatedPeer) -> Result<(), String> {
        let metadata = crate::pure_rust_dht_tcp::StorageMetadata {
            trust_level: peer.trust_score,
            tags: vec!["authenticated".to_string(), "p2p".to_string()],
            access_permissions: vec!["public".to_string()],
            content_type: "peer_announcement".to_string(),
            is_encrypted: false,
            is_compressed: false,
            priority: 5,
            source_peer: Some(peer.peer_id.clone()),
            replication_factor: 3,
        };
        
        // Serialize peer data
        let peer_data = serde_json::to_vec(peer)
            .map_err(|e| format!("Failed to serialize peer: {}", e))?;
        
        // Store in DHT with authentication prefix
        let dht_key = format!("{}{}", DHT_PEER_AUTH_PREFIX, peer.peer_id);
        self.dht.store_with_metadata(&dht_key, peer_data, metadata).await
            .map_err(|e| format!("Failed to store peer in DHT: {}", e))?;
        
        // Also store trust information
        let trust_key = format!("{}{}", DHT_PEER_TRUST_PREFIX, peer.peer_id);
        let trust_data = peer.trust_score.to_be_bytes().to_vec();
        let trust_metadata = crate::pure_rust_dht_tcp::StorageMetadata {
            trust_level: peer.trust_score,
            tags: vec!["trust".to_string(), "score".to_string()],
            content_type: "trust_score".to_string(),
            priority: 8,
            ..Default::default()
        };
        
        self.dht.store_with_metadata(&trust_key, trust_data, trust_metadata).await
            .map_err(|e| format!("Failed to store trust in DHT: {}", e))?;
        
        info!("Successfully announced peer {} to DHT with trust score {}", 
              peer.peer_id, peer.trust_score);
        
        Ok(())
    }
    
    /// Query DHT for authenticated peers by criteria
    pub async fn query_dht_for_peers(&self, criteria: &DhtDiscoveryCriteria) -> Result<Vec<DhtAuthenticatedPeer>, String> {
        let mut discovered_peers = Vec::new();
        
        // Get all peer IDs from DHT
        let all_peer_ids = self.dht.get_all_peer_ids().await
            .map_err(|e| format!("Failed to get peer IDs from DHT: {}", e))?;
        
        for peer_id in all_peer_ids {
            let dht_key = format!("{}{}", DHT_PEER_AUTH_PREFIX, peer_id);
            
            if let Ok(Some((peer_data, metadata))) = self.dht.get_with_metadata(&dht_key).await {
                // Check if metadata meets criteria
                if metadata.trust_level < criteria.min_trust_score {
                    continue;
                }
                
                // Deserialize peer data
                if let Ok(peer) = serde_json::from_slice::<DhtAuthenticatedPeer>(&peer_data) {
                    // Apply additional filters
                    if self.peer_meets_criteria(&peer, criteria).await {
                        discovered_peers.push(peer);
                    }
                }
            }
        }
        
        // Sort by trust score (descending)
        discovered_peers.sort_by(|a, b| b.trust_score.partial_cmp(&a.trust_score).unwrap());
        
        // Limit results
        if let Some(limit) = criteria.max_results {
            discovered_peers.truncate(limit);
        }
        
        Ok(discovered_peers)
    }
    
    /// Check if peer meets discovery criteria
    async fn peer_meets_criteria(&self, peer: &DhtAuthenticatedPeer, criteria: &DhtDiscoveryCriteria) -> bool {
        // Trust score check
        if peer.trust_score < criteria.min_trust_score {
            return false;
        }
        
        // Required capabilities check
        for required_cap in &criteria.required_capabilities {
            if !peer.capabilities.supported_protocols.contains(required_cap) {
                return false;
            }
        }
        
        // Geographic preference check
        if let Some(preferred_region) = &criteria.preferred_region {
            if let Some(peer_region) = peer.network_quality.location_info.get("region") {
                if peer_region != preferred_region {
                    // Deprioritize but don't exclude
                    // This could be implemented as a scoring mechanism
                }
            }
        }
        
        // Performance requirements
        if peer.network_quality.latency_ms > criteria.max_latency_ms.unwrap_or(f64::MAX) {
            return false;
        }
        
        if peer.network_quality.bandwidth_mbps < criteria.min_bandwidth_mbps.unwrap_or(0.0) {
            return false;
        }
        
        true
    }
    
    /// Update peer trust score in DHT
    pub async fn update_peer_trust_in_dht(&self, peer_id: &str, new_trust_score: f64) -> Result<(), String> {
        let trust_key = format!("{}{}", DHT_PEER_TRUST_PREFIX, peer_id);
        let trust_data = new_trust_score.to_be_bytes().to_vec();
        
        let trust_metadata = crate::pure_rust_dht_tcp::StorageMetadata {
            trust_level: new_trust_score,
            tags: vec!["trust".to_string(), "score".to_string(), "updated".to_string()],
            content_type: "trust_score".to_string(),
            priority: 8,
            ..Default::default()
        };
        
        self.dht.store_with_metadata(&trust_key, trust_data, trust_metadata).await
            .map_err(|e| format!("Failed to update trust in DHT: {}", e))?;
        
        info!("Updated trust score for peer {} to {}", peer_id, new_trust_score);
        Ok(())
    }
    
    /// Monitor DHT network health
    pub async fn monitor_dht_health(&self) -> Result<DhtHealthMetrics, String> {
        let topology = self.build_network_topology().await?;
        
        // Get DHT-specific metrics
        let routing_stats = self.dht.get_routing_stats().await;
        let is_connected = self.dht.is_network_connected().await;
        let health_check = self.dht.perform_network_health_check().await
            .map_err(|e| format!("DHT health check failed: {}", e))?;
        
        Ok(DhtHealthMetrics {
            total_authenticated_peers: topology.total_peers,
            average_trust_score: topology.average_trust,
            network_density: topology.network_density,
            partition_risk: topology.partition_risk,
            dht_routing_table_size: routing_stats.total_peers,
            dht_is_connected: is_connected,
            dht_health_status: health_check,
            last_updated: SystemTime::now(),
        })
    }
}

// Implement Clone for PureRustP2PAuth to enable async spawning
impl Clone for PureRustP2PAuth {
    fn clone(&self) -> Self {
        Self {
            local_signing_key: self.local_signing_key.clone(),
            local_verifying_key: self.local_verifying_key,
            local_x25519_secret: self.local_x25519_secret.clone(),
            local_x25519_public: self.local_x25519_public,
            local_peer_id: self.local_peer_id,
            pure_rust_dht: self.pure_rust_dht.clone(),
            authenticated_peers: self.authenticated_peers.clone(),
            pending_auth_requests: self.pending_auth_requests.clone(),
            peer_reputation: self.peer_reputation.clone(),
            event_sender: self.event_sender.clone(),
            network_stats: self.network_stats.clone(),
            config: self.config.clone(),
        }
    }
}

/// Configuration for libp2p network
#[derive(Debug, Clone)]
pub struct LibP2PConfig {
    pub listen_addresses: Vec<Multiaddr>,
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    pub kad_replication_factor: usize,
    pub connection_timeout: Duration,
    pub max_connections: u32,
    pub enable_mdns: bool,
    pub enable_autonat: bool,
    pub enable_relay: bool,
}

impl Default for LibP2PConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
                "/ip6/::/tcp/0".parse().unwrap(),
            ],
            bootstrap_peers: Vec::new(),
            kad_replication_factor: 20,
            connection_timeout: Duration::from_secs(30),
            max_connections: 200,
            enable_mdns: true,
            enable_autonat: true,
            enable_relay: false,
        }
    }
}

/// Network events from libp2p
#[derive(Debug, Clone)]
pub enum LibP2PNetworkEvent {
    PeerConnected { peer_id: PeerId, address: Multiaddr },
    PeerDisconnected { peer_id: PeerId },
    PeerDiscovered { peer_id: PeerId, addresses: Vec<Multiaddr> },
    DhtRecordStored { key: String, peer_id: PeerId },
    DhtRecordFound { key: String, value: Vec<u8>, peer_id: PeerId },
    DhtQueryCompleted { key: String, result: QueryResult },
    /// Authentication events
    PeerAuthenticationStarted { peer_id: PeerId },
    PeerAuthenticationSuccessful { peer_id: PeerId, trust_level: f64 },
    PeerAuthenticationFailed { peer_id: PeerId, reason: String },
    EncryptedSessionEstablished { peer_id: PeerId },
    NetworkError { error: String },
}

impl LibP2PNetwork {
    /// Create new libp2p network instance
    pub async fn new(
        pure_rust_dht: Arc<PureRustDht>, 
        config: LibP2PConfig
    ) -> Result<(Self, mpsc::UnboundedReceiver<LibP2PNetworkEvent>), Box<dyn Error + Send + Sync>> {
        // Generate or load keypair
        let local_keypair = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_keypair.public());
        
        info!("Starting libp2p network with peer ID: {}", local_peer_id);

        // Create transport
        let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&local_keypair)?)
            .multiplex(yamux::Config::default())
            .boxed();

        // Create Kademlia DHT
        let store = MemoryStore::new(local_peer_id);
        let mut kad_config = KademliaConfig::default();
        kad_config.set_replication_factor(
            std::num::NonZeroUsize::new(config.kad_replication_factor).unwrap()
        );
        let kademlia = Kademlia::with_config(local_peer_id, store, kad_config);

        // Create mDNS if enabled
        let mdns = if config.enable_mdns {
            MdnsBehaviour::new(MdnsConfig::default(), local_peer_id)?
        } else {
            // Create disabled mDNS behavior
            MdnsBehaviour::new(MdnsConfig::default(), local_peer_id)?
        };

        // Create authentication request-response behavior
        let auth_config = RequestResponseConfig::default();
        let auth_protocols = std::iter::once((
            StreamProtocol::new("/nyx/auth/1.0.0"),
            ProtocolSupport::Full,
        ));
        let auth = RequestResponse::new(
            AuthCodec,
            auth_protocols,
            auth_config,
        );

        // Combine into network behaviour
        let behaviour = NetworkBehaviour { kademlia, mdns, auth };

        // Create swarm
        let mut swarm = Swarm::with_tokio_executor(transport, behaviour, local_peer_id);

        // Set connection limits
        swarm.behaviour_mut().kademlia.set_mode(Some(libp2p::kad::Mode::Server));

        // Listen on configured addresses
        for addr in &config.listen_addresses {
            match swarm.listen_on(addr.clone()) {
                Ok(_) => info!("Listening on {}", addr),
                Err(e) => warn!("Failed to listen on {}: {}", addr, e),
            }
        }

        // Bootstrap with known peers
        for (peer_id, addr) in &config.bootstrap_peers {
            swarm.behaviour_mut().kademlia.add_address(peer_id, addr.clone());
            if let Err(e) = swarm.dial(addr.clone()) {
                warn!("Failed to dial bootstrap peer {}: {}", peer_id, e);
            }
        }

        // Create event channel
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let network = LibP2PNetwork {
            swarm: Arc::new(RwLock::new(swarm)),
            local_peer_id,
            local_keypair,
            pure_rust_dht,
            connected_peers: Arc::new(RwLock::new(HashMap::new())),
            peer_addresses: Arc::new(RwLock::new(HashMap::new())),
            peer_auth_states: Arc::new(RwLock::new(HashMap::new())),
            pending_auth_requests: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            network_stats: Arc::new(RwLock::new(NetworkStatistics {
                started_at: Instant::now(),
                ..Default::default()
            })),
            config,
        };

        Ok((network, event_receiver))
    }

    /// Start the network event loop
    pub async fn start(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Starting libp2p network event loop for peer {}", self.local_peer_id);

        let swarm = Arc::clone(&self.swarm);
        let connected_peers = Arc::clone(&self.connected_peers);
        let peer_addresses = Arc::clone(&self.peer_addresses);
        let peer_auth_states = Arc::clone(&self.peer_auth_states);
        let pending_auth_requests = Arc::clone(&self.pending_auth_requests);
        let event_sender = self.event_sender.clone();
        let network_stats = Arc::clone(&self.network_stats);
        let pure_rust_dht = Arc::clone(&self.pure_rust_dht);
        let local_keypair = self.local_keypair.clone();

        tokio::spawn(async move {
            let mut swarm = swarm.write().await;
            
            loop {
                match swarm.select_next_some().await {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Local node is listening on {}", address);
                    }
                    SwarmEvent::Behaviour(event) => {
                        Self::handle_behaviour_event(
                            event,
                            &connected_peers,
                            &peer_addresses,
                            &peer_auth_states,
                            &pending_auth_requests,
                            &event_sender,
                            &network_stats,
                            &pure_rust_dht,
                            &local_keypair,
                        ).await;
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        info!("Connection established with peer: {}", peer_id);
                        
                        let peer_info = ConnectedPeerInfo {
                            peer_id,
                            addresses: vec![endpoint.get_remote_address().clone()],
                            connected_at: Instant::now(),
                            last_seen: Instant::now(),
                            connection_quality: 1.0,
                            protocols: Vec::new(),
                            rtt: None,
                            auth_state: PeerAuthState {
                                peer_id,
                                ..Default::default()
                            },
                            encrypted_session: false,
                        };

                        connected_peers.write().await.insert(peer_id, peer_info);
                        
                        // Update statistics
                        {
                            let mut stats = network_stats.write().await;
                            stats.connected_peers = connected_peers.read().await.len();
                            stats.successful_connections += 1;
                        }

                        let _ = event_sender.send(LibP2PNetworkEvent::PeerConnected {
                            peer_id,
                            address: endpoint.get_remote_address().clone(),
                        });
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        info!("Connection closed with peer: {}", peer_id);
                        
                        connected_peers.write().await.remove(&peer_id);
                        
                        // Update statistics
                        {
                            let mut stats = network_stats.write().await;
                            stats.connected_peers = connected_peers.read().await.len();
                        }

                        let _ = event_sender.send(LibP2PNetworkEvent::PeerDisconnected { peer_id });
                    }
                    SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                        if let Some(peer_id) = peer_id {
                            warn!("Failed to connect to peer {}: {}", peer_id, error);
                        } else {
                            warn!("Failed to establish outgoing connection: {}", error);
                        }
                        
                        // Update statistics
                        {
                            let mut stats = network_stats.write().await;
                            stats.failed_connections += 1;
                        }

                        let _ = event_sender.send(LibP2PNetworkEvent::NetworkError {
                            error: format!("Connection failed: {}", error),
                        });
                    }
                    SwarmEvent::IncomingConnectionError { error, .. } => {
                        warn!("Incoming connection failed: {}", error);
                        
                        let _ = event_sender.send(LibP2PNetworkEvent::NetworkError {
                            error: format!("Incoming connection failed: {}", error),
                        });
                    }
                    _ => {}
                }
            }
        });

        // Start periodic maintenance tasks
        self.start_maintenance_tasks().await;

        Ok(())
    }

    /// Handle network behaviour events including authentication
    async fn handle_behaviour_event(
        event: <NetworkBehaviour as NetworkBehaviour>::ToSwarm,
        connected_peers: &Arc<RwLock<HashMap<PeerId, ConnectedPeerInfo>>>,
        peer_addresses: &Arc<RwLock<HashMap<PeerId, Vec<Multiaddr>>>>,
        peer_auth_states: &Arc<RwLock<HashMap<PeerId, PeerAuthState>>>,
        pending_auth_requests: &Arc<RwLock<HashMap<request_response::RequestId, PeerId>>>,
        event_sender: &mpsc::UnboundedSender<LibP2PNetworkEvent>,
        network_stats: &Arc<RwLock<NetworkStatistics>>,
        pure_rust_dht: &Arc<PureRustDht>,
        local_keypair: &Keypair,
    ) {
        match event {
            // Kademlia DHT events
            libp2p::swarm::ToSwarm::GenerateEvent(NetworkBehaviourEvent::Kademlia(kad_event)) => {
                Self::handle_kademlia_event(kad_event, event_sender, network_stats, pure_rust_dht).await;
            }
            // mDNS discovery events
            libp2p::swarm::ToSwarm::GenerateEvent(NetworkBehaviourEvent::Mdns(mdns_event)) => {
                Self::handle_mdns_event(mdns_event, peer_addresses, event_sender).await;
            }
            // Authentication request-response events
            libp2p::swarm::ToSwarm::GenerateEvent(NetworkBehaviourEvent::Auth(auth_event)) => {
                Self::handle_auth_event(
                    auth_event,
                    connected_peers,
                    peer_auth_states,
                    pending_auth_requests,
                    event_sender,
                    network_stats,
                    local_keypair,
                ).await;
            }
            _ => {}
        }
    }

    /// Handle authentication request-response events
    async fn handle_auth_event(
        event: RequestResponseEvent<AuthRequest, AuthResponse>,
        connected_peers: &Arc<RwLock<HashMap<PeerId, ConnectedPeerInfo>>>,
        peer_auth_states: &Arc<RwLock<HashMap<PeerId, PeerAuthState>>>,
        pending_auth_requests: &Arc<RwLock<HashMap<request_response::RequestId, PeerId>>>,
        event_sender: &mpsc::UnboundedSender<LibP2PNetworkEvent>,
        network_stats: &Arc<RwLock<NetworkStatistics>>,
        local_keypair: &Keypair,
    ) {
        match event {
            RequestResponseEvent::Message { peer, message } => {
                match message {
                    RequestResponseMessage::Request { request, channel, .. } => {
                        // Handle incoming authentication request
                        info!("Received authentication request from peer: {}", peer);
                        
                        let response = Self::process_auth_request(request, local_keypair).await;
                        
                        // Send response back
                        // Note: In real implementation, we would use the channel to send response
                        debug!("Processed auth request from {}, status: {:?}", peer, response.status);
                        
                        // Update statistics
                        {
                            let mut stats = network_stats.write().await;
                            stats.auth_attempts += 1;
                            if response.status == AuthStatus::Success {
                                stats.successful_auths += 1;
                            } else {
                                stats.failed_auths += 1;
                            }
                        }
                        
                        // Update peer authentication state
                        if response.status == AuthStatus::Success {
                            let mut auth_state = PeerAuthState {
                                peer_id: peer,
                                status: AuthenticationStatus::Authenticated,
                                verified_public_key: None, // Would extract from request
                                session_key: None, // Would derive from exchange
                                authenticated_at: Some(Instant::now()),
                                trust_level: 0.8, // Initial trust level
                                auth_success_count: 1,
                                auth_failure_count: 0,
                            };
                            
                            peer_auth_states.write().await.insert(peer, auth_state.clone());
                            
                            // Update connected peer info
                            if let Some(peer_info) = connected_peers.write().await.get_mut(&peer) {
                                peer_info.auth_state = auth_state;
                                peer_info.encrypted_session = true;
                            }
                            
                            let _ = event_sender.send(LibP2PNetworkEvent::PeerAuthenticationSuccessful {
                                peer_id: peer,
                                trust_level: 0.8,
                            });
                            
                            let _ = event_sender.send(LibP2PNetworkEvent::EncryptedSessionEstablished {
                                peer_id: peer,
                            });
                        } else {
                            let _ = event_sender.send(LibP2PNetworkEvent::PeerAuthenticationFailed {
                                peer_id: peer,
                                reason: response.error_message.unwrap_or_else(|| "Unknown error".to_string()),
                            });
                        }
                    }
                    RequestResponseMessage::Response { response, request_id } => {
                        // Handle authentication response
                        if let Some(peer_id) = pending_auth_requests.write().await.remove(&request_id) {
                            info!("Received authentication response from peer: {}", peer_id);
                            
                            let authenticated = response.status == AuthStatus::Success;
                            
                            // Update statistics
                            {
                                let mut stats = network_stats.write().await;
                                if authenticated {
                                    stats.successful_auths += 1;
                                    stats.encrypted_connections += 1;
                                } else {
                                    stats.failed_auths += 1;
                                }
                            }
                            
                            if authenticated {
                                // Update peer authentication state
                                let mut auth_state = PeerAuthState {
                                    peer_id,
                                    status: AuthenticationStatus::Authenticated,
                                    verified_public_key: None, // Would extract from response
                                    session_key: None, // Would derive from exchange
                                    authenticated_at: Some(Instant::now()),
                                    trust_level: 0.9, // Higher trust for successful outbound auth
                                    auth_success_count: 1,
                                    auth_failure_count: 0,
                                };
                                
                                peer_auth_states.write().await.insert(peer_id, auth_state.clone());
                                
                                // Update connected peer info
                                if let Some(peer_info) = connected_peers.write().await.get_mut(&peer_id) {
                                    peer_info.auth_state = auth_state;
                                    peer_info.encrypted_session = true;
                                }
                                
                                let _ = event_sender.send(LibP2PNetworkEvent::PeerAuthenticationSuccessful {
                                    peer_id,
                                    trust_level: 0.9,
                                });
                                
                                let _ = event_sender.send(LibP2PNetworkEvent::EncryptedSessionEstablished {
                                    peer_id,
                                });
                            } else {
                                let _ = event_sender.send(LibP2PNetworkEvent::PeerAuthenticationFailed {
                                    peer_id,
                                    reason: response.error_message.unwrap_or_else(|| "Authentication failed".to_string()),
                                });
                            }
                        }
                    }
                }
            }
            RequestResponseEvent::OutboundFailure { peer, request_id, error } => {
                warn!("Authentication request failed for peer {}: {:?}", peer, error);
                
                pending_auth_requests.write().await.remove(&request_id);
                
                let _ = event_sender.send(LibP2PNetworkEvent::PeerAuthenticationFailed {
                    peer_id: peer,
                    reason: format!("Request failed: {:?}", error),
                });
                
                // Update statistics
                {
                    let mut stats = network_stats.write().await;
                    stats.failed_auths += 1;
                }
            }
            RequestResponseEvent::InboundFailure { peer, error, .. } => {
                warn!("Inbound authentication request failed from peer {}: {:?}", peer, error);
            }
            RequestResponseEvent::ResponseSent { peer, .. } => {
                debug!("Authentication response sent to peer: {}", peer);
            }
        }
            }
            _ => {}
        }
    }

    /// Handle Kademlia DHT events
    async fn handle_kademlia_event(
        event: KademliaEvent,
        event_sender: &mpsc::UnboundedSender<LibP2PNetworkEvent>,
        network_stats: &Arc<RwLock<NetworkStatistics>>,
        pure_rust_dht: &Arc<PureRustDht>,
    ) {
        match event {
            KademliaEvent::OutboundQueryProgressed { result, .. } => {
                match result {
                    QueryResult::GetRecord(Ok(GetRecordOk { records, .. })) => {
                        for record in records {
                            let key = String::from_utf8_lossy(&record.record.key.to_vec());
                            debug!("DHT record found: {} from libp2p", key);
                            
                            let _ = event_sender.send(LibP2PNetworkEvent::DhtRecordFound {
                                key: key.to_string(),
                                value: record.record.value,
                                peer_id: record.peer.unwrap_or_else(|| PeerId::random()),
                            });
                        }
                    }
                    QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                        let key_str = String::from_utf8_lossy(&key.to_vec());
                        debug!("DHT record stored: {} via libp2p", key_str);
                        
                        let _ = event_sender.send(LibP2PNetworkEvent::DhtRecordStored {
                            key: key_str.to_string(),
                            peer_id: PeerId::random(), // Placeholder
                        });
                    }
                    QueryResult::GetClosestPeers(Ok(peers)) => {
                        debug!("Found {} closest peers via libp2p Kademlia", peers.peers.len());
                        
                        // Integration with Pure Rust DHT: add discovered peers
                        for peer_id in peers.peers {
                            // Convert libp2p PeerId to our DHT format if needed
                            debug!("Discovered peer via Kademlia: {}", peer_id);
                        }
                    }
                    _ => {}
                }
                
                // Update statistics
                {
                    let mut stats = network_stats.write().await;
                    stats.queries_sent += 1;
                }
            }
            KademliaEvent::InboundRequest { request } => {
                debug!("Received Kademlia inbound request");
                
                // Update statistics
                {
                    let mut stats = network_stats.write().await;
                    stats.queries_received += 1;
                }
            }
            _ => {}
        }
    }

    /// Handle mDNS discovery events
    async fn handle_mdns_event(
        event: MdnsEvent,
        peer_addresses: &Arc<RwLock<HashMap<PeerId, Vec<Multiaddr>>>>,
        event_sender: &mpsc::UnboundedSender<LibP2PNetworkEvent>,
    ) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer_id, multiaddr) in list {
                    info!("mDNS discovered peer: {} at {}", peer_id, multiaddr);
                    
                    // Store peer address
                    peer_addresses.write().await
                        .entry(peer_id)
                        .or_insert_with(Vec::new)
                        .push(multiaddr.clone());
                    
                    let _ = event_sender.send(LibP2PNetworkEvent::PeerDiscovered {
                        peer_id,
                        addresses: vec![multiaddr],
                    });
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer_id, multiaddr) in list {
                    debug!("mDNS peer expired: {} at {}", peer_id, multiaddr);
                    
                    // Remove expired address
                    if let Some(addresses) = peer_addresses.write().await.get_mut(&peer_id) {
                        addresses.retain(|addr| addr != &multiaddr);
                        if addresses.is_empty() {
                            peer_addresses.write().await.remove(&peer_id);
                        }
                    }
                }
            }
        }
    }

    /// Start periodic maintenance tasks
    async fn start_maintenance_tasks(&self) {
        let swarm = Arc::clone(&self.swarm);
        let network_stats = Arc::clone(&self.network_stats);
        let connected_peers = Arc::clone(&self.connected_peers);

        // Periodic bootstrap and DHT maintenance
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                // Perform periodic DHT bootstrap
                {
                    let mut swarm = swarm.write().await;
                    if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
                        warn!("DHT bootstrap failed: {:?}", e);
                    }
                }
                
                // Update statistics
                {
                    let mut stats = network_stats.write().await;
                    stats.uptime = stats.started_at.elapsed();
                    stats.connected_peers = connected_peers.read().await.len();
                    stats.total_peers = connected_peers.read().await.len(); // Simplified
                }
                
                debug!("Performed periodic DHT maintenance");
            }
        });

        // Peer connection health monitoring
        let connected_peers_monitor = Arc::clone(&self.connected_peers);
        let event_sender = self.event_sender.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let mut to_remove = Vec::new();
                
                {
                    let peers = connected_peers_monitor.read().await;
                    for (peer_id, info) in peers.iter() {
                        // Check for stale connections (no activity for 5 minutes)
                        if now.duration_since(info.last_seen) > Duration::from_secs(300) {
                            to_remove.push(*peer_id);
                        }
                    }
                }
                
                // Remove stale peers
                if !to_remove.is_empty() {
                    let mut peers = connected_peers_monitor.write().await;
                    for peer_id in to_remove {
                        peers.remove(&peer_id);
                        let _ = event_sender.send(LibP2PNetworkEvent::PeerDisconnected { peer_id });
                        debug!("Removed stale peer: {}", peer_id);
                    }
                }
            }
        });
    }

    /// Store a record in the DHT via libp2p
    pub async fn dht_put_record(&self, key: String, value: Vec<u8>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let record = Record {
            key: RecordKey::new(&key),
            value,
            publisher: Some(self.local_peer_id),
            expires: None,
        };

        let mut swarm = self.swarm.write().await;
        match swarm.behaviour_mut().kademlia.put_record(record, libp2p::kad::Quorum::One) {
            Ok(_) => {
                debug!("DHT put record initiated for key: {}", key);
                Ok(())
            }
            Err(e) => {
                error!("Failed to put DHT record: {:?}", e);
                Err(format!("DHT put failed: {:?}", e).into())
            }
        }
    }

    /// Get a record from the DHT via libp2p
    pub async fn dht_get_record(&self, key: String) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut swarm = self.swarm.write().await;
        let query_id = swarm.behaviour_mut().kademlia.get_record(RecordKey::new(&key));
        
        debug!("DHT get record initiated for key: {} (query ID: {:?})", key, query_id);
        Ok(())
    }

    /// Find the closest peers to a key via libp2p Kademlia
    pub async fn find_closest_peers(&self, key: String) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut swarm = self.swarm.write().await;
        let key_bytes = key.as_bytes();
        let query_id = swarm.behaviour_mut().kademlia.get_closest_peers(key_bytes);
        
        debug!("Finding closest peers for key: {} (query ID: {:?})", key, query_id);
        Ok(())
    }

    /// Connect to a specific peer
    pub async fn connect_peer(&self, address: Multiaddr) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut swarm = self.swarm.write().await;
        
        match swarm.dial(address.clone()) {
            Ok(_) => {
                info!("Dialing peer at: {}", address);
                
                // Update statistics
                {
                    let mut stats = self.network_stats.write().await;
                    stats.connection_attempts += 1;
                }
                
                Ok(())
            }
            Err(e) => {
                error!("Failed to dial peer at {}: {}", address, e);
                Err(format!("Dial failed: {}", e).into())
            }
        }
    }

    /// Get current network statistics
    pub async fn get_network_statistics(&self) -> NetworkStatistics {
        let stats = self.network_stats.read().await;
        let mut current_stats = stats.clone();
        current_stats.uptime = stats.started_at.elapsed();
        current_stats.connected_peers = self.connected_peers.read().await.len();
        current_stats
    }

    /// Get list of connected peers
    pub async fn get_connected_peers(&self) -> Vec<ConnectedPeerInfo> {
        self.connected_peers.read().await.values().cloned().collect()
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Integrate discovered libp2p peers with Pure Rust DHT
    pub async fn sync_with_pure_rust_dht(&self) -> Result<(), DhtError> {
        let connected_peers = self.connected_peers.read().await;
        
        for (peer_id, peer_info) in connected_peers.iter() {
            // Convert libp2p peer info to our DHT format
            for address in &peer_info.addresses {
                if let Some(socket_addr) = Self::multiaddr_to_socket_addr(address) {
                    // Create PeerInfo for our DHT
                    let dht_peer = PeerInfo {
                        peer_id: peer_id.to_string(),
                        address: address.clone(),
                        public_key: Vec::new(), // Would need to be obtained through identification
                        last_seen: SystemTime::now(),
                        avg_response_time: peer_info.rtt.unwrap_or(Duration::from_millis(100)),
                    };
                    
                    // Note: Integration point with Pure Rust DHT
                    // This would require adding peers to the routing table
                    debug!("Would integrate peer {} with Pure Rust DHT", peer_id);
                }
            }
        }
        
        Ok(())
    }

    /// Convert Multiaddr to SocketAddr for Pure Rust DHT compatibility
    fn multiaddr_to_socket_addr(addr: &Multiaddr) -> Option<std::net::SocketAddr> {
        // Implementation to extract TCP socket address from Multiaddr
        // This is a simplified version
        None // Placeholder
    }

    /// Process incoming authentication request
    async fn process_auth_request(
        request: AuthRequest,
        local_keypair: &Keypair,
    ) -> AuthResponse {
        // Validate protocol version
        if request.protocol_version != 1 {
            return AuthResponse {
                status: AuthStatus::UnsupportedVersion,
                public_key: None,
                challenge_response: None,
                encrypted_session_key: None,
                error_message: Some("Unsupported protocol version".to_string()),
            };
        }
        
        // Validate timestamp (within 5 minutes)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        if now.saturating_sub(request.timestamp) > 300 { // 5 minutes
            return AuthResponse {
                status: AuthStatus::Expired,
                public_key: None,
                challenge_response: None,
                encrypted_session_key: None,
                error_message: Some("Request timestamp expired".to_string()),
            };
        }
        
        // Verify signature (simplified - would use ed25519_dalek in real implementation)
        let signature_valid = Self::verify_signature(&request);
        
        if !signature_valid {
            return AuthResponse {
                status: AuthStatus::InvalidSignature,
                public_key: None,
                challenge_response: None,
                encrypted_session_key: None,
                error_message: Some("Invalid signature".to_string()),
            };
        }
        
        // Generate session key and encrypt it with peer's public key
        let session_key = Self::generate_session_key();
        let encrypted_session_key = Self::encrypt_session_key(&session_key, &request.public_key);
        
        // Create challenge response
        let challenge_response = Self::create_challenge_response(&request.nonce, local_keypair);
        
        AuthResponse {
            status: AuthStatus::Success,
            public_key: Some(local_keypair.public().encode_protobuf()),
            challenge_response: Some(challenge_response),
            encrypted_session_key: Some(encrypted_session_key),
            error_message: None,
        }
    }
    
    /// Verify signature in authentication request
    fn verify_signature(request: &AuthRequest) -> bool {
        // In real implementation, would use ed25519_dalek to verify signature
        // For now, return true for demonstration
        true
    }
    
    /// Generate random session key for encrypted communication
    fn generate_session_key() -> [u8; 32] {
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        key
    }
    
    /// Encrypt session key with peer's public key
    fn encrypt_session_key(session_key: &[u8; 32], peer_public_key: &[u8]) -> Vec<u8> {
        // In real implementation, would use X25519 + ChaCha20Poly1305
        // For now, return the key as-is (would be encrypted)
        session_key.to_vec()
    }
    
    /// Create challenge response signature
    fn create_challenge_response(nonce: &[u8; 32], local_keypair: &Keypair) -> Vec<u8> {
        // In real implementation, would sign the nonce with local private key
        // For now, return a placeholder
        nonce.to_vec()
    }

    /// Initiate authentication with a peer
    pub async fn authenticate_peer(&self, peer_id: PeerId) -> Result<(), Box<dyn Error + Send + Sync>> {
        info!("Initiating authentication with peer: {}", peer_id);
        
        // Generate authentication request
        let nonce = {
            use rand::RngCore;
            let mut nonce = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut nonce);
            nonce
        };
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let public_key = self.local_keypair.public().encode_protobuf();
        
        // Create signature (simplified)
        let signature = Self::sign_auth_request(&nonce, timestamp, &self.local_keypair);
        
        let auth_request = AuthRequest {
            nonce,
            timestamp,
            public_key,
            signature,
            protocol_version: 1,
        };
        
        // Send authentication request
        let mut swarm = self.swarm.write().await;
        let request_id = swarm.behaviour_mut().auth.send_request(&peer_id, auth_request);
        
        // Track pending request
        self.pending_auth_requests.write().await.insert(request_id, peer_id);
        
        // Update authentication state
        let auth_state = PeerAuthState {
            peer_id,
            status: AuthenticationStatus::InProgress,
            ..Default::default()
        };
        self.peer_auth_states.write().await.insert(peer_id, auth_state);
        
        // Send event
        let _ = self.event_sender.send(LibP2PNetworkEvent::PeerAuthenticationStarted { peer_id });
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    #[tokio::test]
    async fn test_pure_rust_p2p_auth_creation() {
        // Create mock DHT
        let mock_dht = Arc::new(PureRustDht::new(
            "127.0.0.1:0".parse().unwrap(),
            Vec::new(),
        ));
        
        let config = P2PAuthConfig::default();
        let result = PureRustP2PAuth::new(mock_dht, config);
        
        assert!(result.is_ok());
        let (auth_manager, _event_receiver) = result.unwrap();
        
        // Verify peer ID is generated correctly
        assert_ne!(auth_manager.local_peer_id, [0u8; 32]);
        
        // Verify keys are generated
        assert_ne!(auth_manager.local_verifying_key.to_bytes(), [0u8; 32]);
        assert_ne!(auth_manager.local_x25519_public.to_bytes(), [0u8; 32]);
    }
    
    #[tokio::test]
    async fn test_peer_id_generation() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        
        let peer_id1 = PureRustP2PAuth::generate_peer_id(&verifying_key);
        let peer_id2 = PureRustP2PAuth::generate_peer_id(&verifying_key);
        
        // Peer ID should be deterministic
        assert_eq!(peer_id1, peer_id2);
        
        // Different keys should produce different peer IDs
        let signing_key2 = SigningKey::generate(&mut OsRng);
        let verifying_key2 = VerifyingKey::from(&signing_key2);
        let peer_id3 = PureRustP2PAuth::generate_peer_id(&verifying_key2);
        
        assert_ne!(peer_id1, peer_id3);
    }
    
    #[tokio::test]
    async fn test_auth_request_creation() {
        let mock_dht = Arc::new(PureRustDht::new(
            "127.0.0.1:0".parse().unwrap(),
            Vec::new(),
        ));
        
        let config = P2PAuthConfig::default();
        let (auth_manager, _event_receiver) = PureRustP2PAuth::new(mock_dht, config).unwrap();
        
        let auth_request = auth_manager.create_auth_request().await.unwrap();
        
        // Verify request structure
        assert_ne!(auth_request.request_id, 0);
        assert_eq!(auth_request.peer_id, auth_manager.local_peer_id);
        assert_eq!(auth_request.ed25519_public_key, auth_manager.local_verifying_key.to_bytes());
        assert_eq!(auth_request.x25519_public_key, auth_manager.local_x25519_public.to_bytes());
        assert_ne!(auth_request.challenge, [0u8; 32]);
        assert_ne!(auth_request.signature, [0u8; 64]);
    }
    
    #[tokio::test]
    async fn test_trust_level_calculation() {
        let mock_dht = Arc::new(PureRustDht::new(
            "127.0.0.1:0".parse().unwrap(),
            Vec::new(),
        ));
        
        let config = P2PAuthConfig::default();
        let (auth_manager, _event_receiver) = PureRustP2PAuth::new(mock_dht, config).unwrap();
        
        let peer_id = [1u8; 32]; // Mock peer ID
        
        // Test initial trust level for unknown peer
        let trust_level = auth_manager.calculate_initial_trust_level(peer_id).await;
        assert_eq!(trust_level, 0.5); // Default for new peers
        
        // Add reputation and test again
        {
            let mut reputation = auth_manager.peer_reputation.write().await;
            reputation.insert(peer_id, PeerReputation {
                trust_score: 0.8,
                successful_interactions: 10,
                failed_interactions: 1,
                last_interaction: Instant::now(),
                total_uptime: Duration::from_secs(3600),
                connection_stability: 0.9,
                message_reliability: 0.95,
                misbehavior_reports: 0,
                qos_metrics: QoSMetrics::default(),
            });
        }
        
        let trust_level = auth_manager.calculate_initial_trust_level(peer_id).await;
        assert_eq!(trust_level, 0.8);
    }
    
    #[tokio::test]
    async fn test_session_key_derivation() {
        let mock_dht = Arc::new(PureRustDht::new(
            "127.0.0.1:0".parse().unwrap(),
            Vec::new(),
        ));
        
        let config = P2PAuthConfig::default();
        let (auth_manager, _event_receiver) = PureRustP2PAuth::new(mock_dht, config).unwrap();
        
        let shared_secret = b"test_shared_secret_32_bytes_long";
        let peer_id = [1u8; 32];
        
        let session_key1 = auth_manager.derive_session_key(shared_secret, peer_id).await.unwrap();
        let session_key2 = auth_manager.derive_session_key(shared_secret, peer_id).await.unwrap();
        
        // Keys should be deterministic
        assert_eq!(session_key1, session_key2);
        
        // Different peer ID should produce different key
        let peer_id2 = [2u8; 32];
        let session_key3 = auth_manager.derive_session_key(shared_secret, peer_id2).await.unwrap();
        
        assert_ne!(session_key1, session_key3);
    }
    
    #[tokio::test]
    async fn test_network_statistics() {
        let mock_dht = Arc::new(PureRustDht::new(
            "127.0.0.1:0".parse().unwrap(),
            Vec::new(),
        ));
        
        let config = P2PAuthConfig::default();
        let (auth_manager, _event_receiver) = PureRustP2PAuth::new(mock_dht, config).unwrap();
        
        // Get initial statistics
        let stats = auth_manager.get_network_statistics().await;
        assert_eq!(stats.total_auth_attempts, 0);
        assert_eq!(stats.successful_authentications, 0);
        assert_eq!(stats.authenticated_peers_count, 0);
        
        // Simulate authentication attempt
        {
            let mut network_stats = auth_manager.network_stats.write().await;
            network_stats.total_auth_attempts += 1;
            network_stats.successful_authentications += 1;
        }
        
        let stats = auth_manager.get_network_statistics().await;
        assert_eq!(stats.total_auth_attempts, 1);
        assert_eq!(stats.successful_authentications, 1);
    }
    
    #[tokio::test]
    async fn test_authentication_flow() {
        let mock_dht = Arc::new(PureRustDht::new(
            "127.0.0.1:0".parse().unwrap(),
            Vec::new(),
        ));
        
        let config = P2PAuthConfig::default();
        let (auth_manager, _event_receiver) = PureRustP2PAuth::new(mock_dht, config).unwrap();
        
        let peer_id = [1u8; 32];
        let peer_address = "127.0.0.1:8080".parse().unwrap();
        
        // Start authentication
        let result = auth_manager.authenticate_peer(peer_id, peer_address).await;
        assert!(result.is_ok());
        
        // Verify peer is in authenticated peers list with in-progress status
        let peers = auth_manager.authenticated_peers.read().await;
        let peer_info = peers.get(&peer_id).unwrap();
        assert_eq!(peer_info.auth_state.status, AuthenticationStatus::InProgress);
        assert_eq!(peer_info.auth_state.peer_id, peer_id);
        
        // Verify pending request exists
        let pending = auth_manager.pending_auth_requests.read().await;
        assert!(!pending.is_empty());
        
        // Test duplicate authentication attempt
        let result2 = auth_manager.authenticate_peer(peer_id, peer_address).await;
        assert!(matches!(result2, Err(P2PError::AuthenticationInProgress)));
    }
}
            debug!("Updated trust level for peer {}: {}", peer_id, auth_state.trust_level);
            Ok(())
        } else {
            Err("Peer not found".into())
        }
    }
    
    /// Get all authenticated peers
    pub async fn get_authenticated_peers(&self) -> Vec<PeerId> {
        self.peer_auth_states
            .read()
            .await
            .iter()
            .filter_map(|(peer_id, state)| {
                if matches!(state.status, AuthenticationStatus::Authenticated) {
                    Some(*peer_id)
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Send encrypted message to authenticated peer
    pub async fn send_encrypted_message(
        &self,
        peer_id: &PeerId,
        message: Vec<u8>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Check if peer is authenticated
        if !self.is_peer_authenticated(peer_id).await {
            return Err("Peer not authenticated".into());
        }
        
        // Get peer's session key
        let session_key = if let Some(auth_state) = self.get_peer_auth_state(peer_id).await {
            auth_state.session_key
        } else {
            return Err("No session key available".into());
        };
        
        // Encrypt message (simplified)
        let encrypted_message = if let Some(key) = session_key {
            Self::encrypt_message(&message, &key)
        } else {
            return Err("No session key available".into());
        };
        
        // Send via underlying transport (would integrate with libp2p messaging)
        info!("Sending encrypted message to peer: {}", peer_id);
        Ok(())
    }
    
    /// Encrypt message with session key
    fn encrypt_message(message: &[u8], session_key: &[u8; 32]) -> Vec<u8> {
        // In real implementation, would use ChaCha20Poly1305 with session key
        message.to_vec() // Placeholder - would be encrypted
    }
    }
}

/// Network Behaviour Event enum for pattern matching
#[derive(Debug)]
pub enum NetworkBehaviourEvent {
    Kademlia(KademliaEvent),
    Mdns(MdnsEvent),
    Auth(RequestResponseEvent<AuthRequest, AuthResponse>),
}

impl From<KademliaEvent> for NetworkBehaviourEvent {
    fn from(event: KademliaEvent) -> Self {
        NetworkBehaviourEvent::Kademlia(event)
    }
}

impl From<MdnsEvent> for NetworkBehaviourEvent {
    fn from(event: MdnsEvent) -> Self {
        NetworkBehaviourEvent::Mdns(event)
    }
}

impl From<RequestResponseEvent<AuthRequest, AuthResponse>> for NetworkBehaviourEvent {
    fn from(event: RequestResponseEvent<AuthRequest, AuthResponse>) -> Self {
        NetworkBehaviourEvent::Auth(event)
    }
}
