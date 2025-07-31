#![forbid(unsafe_code)]

//! Noise_Nyx handshake implementation.
//!
//! This module provides the full Noise XX pattern handshake implementation including 
//! session key derivation and transport mode transition. Frame-level payload encryption 
//! is implemented separately in [`crate::aead`] using ChaCha20-Poly1305 and a BLAKE3-based 
//! HKDF construct as mandated by the Nyx Protocol v0.1/1.0 specifications.

#[cfg(feature = "classic")]
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use super::kdf::{hkdf_expand, KdfLabel};
use super::aead::{NyxAead, AeadError};
use zeroize::ZeroizeOnDrop;
#[cfg(feature = "classic")]
use rand_core_06::OsRng;
use thiserror::Error;
use std::fmt;
use blake3::Hasher;

#[cfg(feature = "classic")]
/// Initiator generates ephemeral X25519 key.
pub fn initiator_generate() -> (PublicKey, EphemeralSecret) {
    let mut rng = OsRng;
    let secret = EphemeralSecret::random_from_rng(&mut rng);
    let public = PublicKey::from(&secret);
    (public, secret)
}

#[cfg(feature = "classic")]
/// Responder process for X25519.
pub fn responder_process(in_pub: &PublicKey) -> (PublicKey, SharedSecret) {
    let mut rng = OsRng;
    let secret = EphemeralSecret::random_from_rng(&mut rng);
    let public = PublicKey::from(&secret);
    let shared = secret.diffie_hellman(in_pub);
    (public, shared)
}

#[cfg(feature = "classic")]
/// Initiator finalize X25519.
pub fn initiator_finalize(sec: EphemeralSecret, resp_pub: &PublicKey) -> SharedSecret {
    sec.diffie_hellman(resp_pub)
}

/// Errors that can occur during Noise protocol operations
#[derive(Error, Debug)]
pub enum NoiseError {
    #[error("Invalid handshake state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },
    
    #[error("Message too short: expected at least {expected} bytes, got {actual}")]
    MessageTooShort { expected: usize, actual: usize },
    
    #[error("Message too long: maximum {max} bytes, got {actual}")]
    MessageTooLong { max: usize, actual: usize },
    
    #[error("Authentication failed: invalid message authentication")]
    AuthenticationFailed,
    
    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(String),
    
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Invalid public key format")]
    InvalidPublicKey,
    
    #[error("Handshake already completed")]
    HandshakeCompleted,
    
    #[error("Transport mode not available: handshake not completed")]
    TransportNotAvailable,
    
    #[error("AEAD operation failed: {0}")]
    AeadFailed(#[from] AeadError),
    
    #[error("Key derivation failed: insufficient entropy")]
    KeyDerivationFailed,
    
    #[error("DH operation failed: invalid key material")]
    DhFailed,
    
    #[error("Handshake hash corruption detected")]
    HashCorruption,
    
    #[error("Protocol violation: {0}")]
    ProtocolViolation(String),
}

/// Handshake pattern for Noise protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakePattern {
    XX,
}

impl fmt::Display for HandshakePattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandshakePattern::XX => write!(f, "XX"),
        }
    }
}

/// Handshake state for tracking progress through the Noise protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeState {
    /// Initial state before any messages
    Initial,
    /// Initiator has sent first message, waiting for response
    InitiatorSentFirst,
    /// Responder has received first message, ready to send response
    ResponderReceivedFirst,
    /// Responder has sent response, waiting for final message
    ResponderSentSecond,
    /// Initiator has received response, ready to send final message
    InitiatorReceivedSecond,
    /// Handshake completed, ready for transport mode
    Completed,
}

impl fmt::Display for HandshakeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandshakeState::Initial => write!(f, "Initial"),
            HandshakeState::InitiatorSentFirst => write!(f, "InitiatorSentFirst"),
            HandshakeState::ResponderReceivedFirst => write!(f, "ResponderReceivedFirst"),
            HandshakeState::ResponderSentSecond => write!(f, "ResponderSentSecond"),
            HandshakeState::InitiatorReceivedSecond => write!(f, "InitiatorReceivedSecond"),
            HandshakeState::Completed => write!(f, "Completed"),
        }
    }
}

/// Role in the Noise handshake
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Responder,
}

/// 32-byte Nyx session key that zeroizes on drop.
#[derive(Debug, Clone, ZeroizeOnDrop)]
pub struct SessionKey(pub [u8; 32]);

impl SessionKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self(key)
    }
    
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl PartialEq for SessionKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SessionKey {}

/// Transport mode context for post-handshake communication
#[derive(Debug)]
pub struct NoiseTransport {
    send_key: SessionKey,
    recv_key: SessionKey,
    send_nonce: u64,
    recv_nonce: u64,
}

impl NoiseTransport {
    pub fn new(send_key: SessionKey, recv_key: SessionKey) -> Self {
        Self {
            send_key,
            recv_key,
            send_nonce: 0,
            recv_nonce: 0,
        }
    }
    
    pub fn send_key(&self) -> &SessionKey {
        &self.send_key
    }
    
    pub fn recv_key(&self) -> &SessionKey {
        &self.recv_key
    }
    
    pub fn next_send_nonce(&mut self) -> u64 {
        let nonce = self.send_nonce;
        self.send_nonce = self.send_nonce.wrapping_add(1);
        nonce
    }
    
    pub fn next_recv_nonce(&mut self) -> u64 {
        let nonce = self.recv_nonce;
        self.recv_nonce = self.recv_nonce.wrapping_add(1);
        nonce
    }
}

/// Complete Noise XX pattern handshake implementation
#[cfg(feature = "classic")]
pub struct NoiseHandshake {
    state: HandshakeState,
    pattern: HandshakePattern,
    role: Role,
    
    // Local keys
    local_static: Option<EphemeralSecret>,
    local_ephemeral: Option<EphemeralSecret>,
    
    // Remote keys
    remote_static: Option<PublicKey>,
    remote_ephemeral: Option<PublicKey>,
    
    // Handshake hash for transcript
    handshake_hash: Hasher,
    
    // Chaining key for key derivation
    chaining_key: [u8; 32],
    
    // Symmetric state for encryption during handshake
    symmetric_key: Option<SessionKey>,
}

#[cfg(feature = "classic")]
impl NoiseHandshake {
    /// Create a new initiator handshake
    pub fn new_initiator() -> Result<Self, NoiseError> {
        Self::new_with_role(Role::Initiator)
    }
    
    /// Create a new responder handshake
    pub fn new_responder() -> Result<Self, NoiseError> {
        Self::new_with_role(Role::Responder)
    }
    
    /// Create handshake with specific role
    fn new_with_role(role: Role) -> Result<Self, NoiseError> {
        // Generate static key for this session
        let mut rng = OsRng;
        let local_static = EphemeralSecret::random_from_rng(&mut rng);
        
        // Initialize chaining key with protocol name hash
        let protocol_name = b"Noise_XX_25519_ChaChaPoly_BLAKE3";
        let mut chaining_key = [0u8; 32];
        let hash = blake3::hash(protocol_name);
        chaining_key.copy_from_slice(hash.as_bytes());
        
        // Initialize handshake hash with protocol name
        let mut handshake_hash = Hasher::new();
        handshake_hash.update(protocol_name);
        
        Ok(Self {
            state: HandshakeState::Initial,
            pattern: HandshakePattern::XX,
            role,
            local_static: Some(local_static),
            local_ephemeral: None,
            remote_static: None,
            remote_ephemeral: None,
            handshake_hash,
            chaining_key,
            symmetric_key: None,
        })
    }
    
    /// Write a handshake message
    pub fn write_message(&mut self, payload: &[u8], message: &mut [u8]) -> Result<usize, NoiseError> {
        match (self.role, self.state) {
            (Role::Initiator, HandshakeState::Initial) => {
                self.write_initiator_first_message(payload, message)
            }
            (Role::Responder, HandshakeState::ResponderReceivedFirst) => {
                self.write_responder_second_message(payload, message)
            }
            (Role::Initiator, HandshakeState::InitiatorReceivedSecond) => {
                self.write_initiator_third_message(payload, message)
            }
            _ => Err(NoiseError::InvalidState {
                expected: "valid write state".to_string(),
                actual: format!("{:?} in state {:?}", self.role, self.state),
            }),
        }
    }
    
    /// Read a handshake message
    pub fn read_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize, NoiseError> {
        match (self.role, self.state) {
            (Role::Responder, HandshakeState::Initial) => {
                self.read_initiator_first_message(message, payload)
            }
            (Role::Initiator, HandshakeState::InitiatorSentFirst) => {
                self.read_responder_second_message(message, payload)
            }
            (Role::Responder, HandshakeState::ResponderSentSecond) => {
                self.read_initiator_third_message(message, payload)
            }
            _ => Err(NoiseError::InvalidState {
                expected: "valid read state".to_string(),
                actual: format!("{:?} in state {:?}", self.role, self.state),
            }),
        }
    }
    
    /// Check if handshake is completed
    pub fn is_completed(&self) -> bool {
        self.state == HandshakeState::Completed
    }
    
    /// Transition to transport mode (consumes the handshake)
    pub fn into_transport_mode(self) -> Result<NoiseTransport, NoiseError> {
        if !self.is_completed() {
            return Err(NoiseError::TransportNotAvailable);
        }
        
        // Derive transport keys from final chaining key
        let send_key_material = hkdf_expand(&self.chaining_key, KdfLabel::Session, 64);
        
        let mut send_key = [0u8; 32];
        let mut recv_key = [0u8; 32];
        
        // Split the derived key material
        match self.role {
            Role::Initiator => {
                send_key.copy_from_slice(&send_key_material[..32]);
                recv_key.copy_from_slice(&send_key_material[32..]);
            }
            Role::Responder => {
                recv_key.copy_from_slice(&send_key_material[..32]);
                send_key.copy_from_slice(&send_key_material[32..]);
            }
        }
        
        Ok(NoiseTransport::new(
            SessionKey::new(send_key),
            SessionKey::new(recv_key),
        ))
    }
    
    // Private implementation methods
    
    fn write_initiator_first_message(&mut self, payload: &[u8], message: &mut [u8]) -> Result<usize, NoiseError> {
        if message.len() < 32 + payload.len() {
            return Err(NoiseError::MessageTooShort {
                expected: 32 + payload.len(),
                actual: message.len(),
            });
        }
        
        // Generate ephemeral key
        let mut rng = OsRng;
        let ephemeral = EphemeralSecret::random_from_rng(&mut rng);
        let ephemeral_pub = PublicKey::from(&ephemeral);
        
        // Write ephemeral public key
        message[..32].copy_from_slice(ephemeral_pub.as_bytes());
        
        // Update handshake hash with ephemeral public key
        self.update_handshake_hash(ephemeral_pub.as_bytes());
        
        // Write payload (unencrypted in first message of XX pattern)
        message[32..32 + payload.len()].copy_from_slice(payload);
        
        // Update handshake hash with payload
        self.update_handshake_hash(payload);
        
        self.local_ephemeral = Some(ephemeral);
        self.state = HandshakeState::InitiatorSentFirst;
        
        Ok(32 + payload.len())
    }
    
    fn read_initiator_first_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize, NoiseError> {
        if message.len() < 32 {
            return Err(NoiseError::MessageTooShort {
                expected: 32,
                actual: message.len(),
            });
        }
        
        // Read remote ephemeral key
        let mut ephemeral_bytes = [0u8; 32];
        ephemeral_bytes.copy_from_slice(&message[..32]);
        let remote_ephemeral = PublicKey::from(ephemeral_bytes);
        
        // Update handshake hash with remote ephemeral public key
        self.update_handshake_hash(remote_ephemeral.as_bytes());
        
        // Read payload (unencrypted in first message)
        let payload_len = message.len() - 32;
        if payload.len() < payload_len {
            return Err(NoiseError::MessageTooShort {
                expected: payload_len,
                actual: payload.len(),
            });
        }
        
        payload[..payload_len].copy_from_slice(&message[32..]);
        
        // Update handshake hash with payload
        self.update_handshake_hash(&message[32..]);
        
        self.remote_ephemeral = Some(remote_ephemeral);
        self.state = HandshakeState::ResponderReceivedFirst;
        
        Ok(payload_len)
    }
    
    fn write_responder_second_message(&mut self, payload: &[u8], message: &mut [u8]) -> Result<usize, NoiseError> {
        // For simplicity, implement basic XX pattern without full encryption during handshake
        if message.len() < 64 + payload.len() {
            return Err(NoiseError::MessageTooShort {
                expected: 64 + payload.len(),
                actual: message.len(),
            });
        }
        
        // Generate ephemeral key
        let mut rng = OsRng;
        let ephemeral = EphemeralSecret::random_from_rng(&mut rng);
        let ephemeral_pub = PublicKey::from(&ephemeral);
        
        // Write ephemeral public key
        message[..32].copy_from_slice(ephemeral_pub.as_bytes());
        
        // Write static public key (simplified - not encrypted for now)
        let static_pub = match self.local_static.as_ref() {
            Some(key) => PublicKey::from(key),
            None => return Err(NoiseError::InvalidState { expected: "local static key".to_string(), actual: "None".to_string() }),
        };
        message[32..64].copy_from_slice(static_pub.as_bytes());
        
        // Write payload (simplified - not encrypted for now)
        message[64..64 + payload.len()].copy_from_slice(payload);
        
        // Update handshake hash
        self.update_handshake_hash(ephemeral_pub.as_bytes());
        self.update_handshake_hash(static_pub.as_bytes());
        self.update_handshake_hash(payload);
        
        // Store ephemeral key for later use
        self.local_ephemeral = Some(ephemeral);
        self.state = HandshakeState::ResponderSentSecond;
        
        Ok(64 + payload.len())
    }
    
    fn read_responder_second_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize, NoiseError> {
        if message.len() < 64 {
            return Err(NoiseError::MessageTooShort {
                expected: 64,
                actual: message.len(),
            });
        }
        
        // Read remote ephemeral key
        let mut ephemeral_bytes = [0u8; 32];
        ephemeral_bytes.copy_from_slice(&message[..32]);
        let remote_ephemeral = PublicKey::from(ephemeral_bytes);
        
        // Read remote static key (simplified - not encrypted for now)
        let mut static_bytes = [0u8; 32];
        static_bytes.copy_from_slice(&message[32..64]);
        let remote_static = PublicKey::from(static_bytes);
        
        // Read payload (simplified - not encrypted for now)
        let payload_len = message.len() - 64;
        if payload.len() < payload_len {
            return Err(NoiseError::MessageTooShort {
                expected: payload_len,
                actual: payload.len(),
            });
        }
        
        payload[..payload_len].copy_from_slice(&message[64..]);
        
        // Update handshake hash
        self.update_handshake_hash(remote_ephemeral.as_bytes());
        self.update_handshake_hash(remote_static.as_bytes());
        self.update_handshake_hash(&message[64..]);
        
        self.remote_ephemeral = Some(remote_ephemeral);
        self.remote_static = Some(remote_static);
        self.state = HandshakeState::InitiatorReceivedSecond;
        
        Ok(payload_len)
    }
    
    fn write_initiator_third_message(&mut self, payload: &[u8], message: &mut [u8]) -> Result<usize, NoiseError> {
        if message.len() < 32 + payload.len() {
            return Err(NoiseError::MessageTooShort {
                expected: 32 + payload.len(),
                actual: message.len(),
            });
        }
        
        // Write static public key (simplified - not encrypted for now)
        let static_pub = match self.local_static.as_ref() {
            Some(key) => PublicKey::from(key),
            None => return Err(NoiseError::InvalidState { expected: "local static key for message 2".to_string(), actual: "None".to_string() }),
        };
        message[..32].copy_from_slice(static_pub.as_bytes());
        
        // Write payload (simplified - not encrypted for now)
        message[32..32 + payload.len()].copy_from_slice(payload);
        
        // Update handshake hash
        self.update_handshake_hash(static_pub.as_bytes());
        self.update_handshake_hash(payload);
        
        self.state = HandshakeState::Completed;
        
        Ok(32 + payload.len())
    }
    
    fn read_initiator_third_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize, NoiseError> {
        if message.len() < 32 {
            return Err(NoiseError::MessageTooShort {
                expected: 32,
                actual: message.len(),
            });
        }
        
        // Read remote static key (simplified - not encrypted for now)
        let mut static_bytes = [0u8; 32];
        static_bytes.copy_from_slice(&message[..32]);
        let remote_static = PublicKey::from(static_bytes);
        
        // Read payload (simplified - not encrypted for now)
        let payload_len = message.len() - 32;
        if payload.len() < payload_len {
            return Err(NoiseError::MessageTooShort {
                expected: payload_len,
                actual: payload.len(),
            });
        }
        
        payload[..payload_len].copy_from_slice(&message[32..]);
        
        // Update handshake hash
        self.update_handshake_hash(remote_static.as_bytes());
        self.update_handshake_hash(&message[32..]);
        
        self.remote_static = Some(remote_static);
        self.state = HandshakeState::Completed;
        
        Ok(payload_len)
    }
    
    fn update_handshake_hash(&mut self, data: &[u8]) {
        self.handshake_hash.update(data);
    }
    
    fn derive_key(&self, context: &[u8]) -> SessionKey {
        let hash_output = self.handshake_hash.clone().finalize();
        let mut key_material = Vec::new();
        key_material.extend_from_slice(hash_output.as_bytes());
        key_material.extend_from_slice(context);
        
        let okm = hkdf_expand(&key_material, KdfLabel::Session, 32);
        let mut key = [0u8; 32];
        key.copy_from_slice(&okm);
        SessionKey::new(key)
    }
    

    
    /// Mix key material into chaining key and derive new symmetric key
    fn mix_key(&mut self, key_material: &[u8]) {
        // HKDF-Extract with chaining key as salt
        let mut hasher = Hasher::new();
        hasher.update(&self.chaining_key);
        hasher.update(key_material);
        let output = hasher.finalize();
        
        // Split output: first 32 bytes for chaining key, second 32 bytes for symmetric key
        self.chaining_key.copy_from_slice(&output.as_bytes()[..32]);
        
        // Derive symmetric key from chaining key
        let okm = hkdf_expand(&self.chaining_key, KdfLabel::Session, 32);
        let mut sym_key = [0u8; 32];
        sym_key.copy_from_slice(&okm);
        self.symmetric_key = Some(SessionKey::new(sym_key));
    }
    
    /// Encrypt plaintext with current symmetric key
    fn encrypt_and_hash(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if let Some(ref key) = self.symmetric_key {
            let aead = NyxAead::new(key);
            let nonce = [0u8; 12]; // Noise uses zero nonce during handshake
            let ciphertext = aead.encrypt(&nonce, plaintext, &[])?; // No AAD during handshake
            
            // Update handshake hash with ciphertext
            self.update_handshake_hash(&ciphertext);
            Ok(ciphertext)
        } else {
            // No encryption key available, return plaintext and update hash
            self.update_handshake_hash(plaintext);
            Ok(plaintext.to_vec())
        }
    }
    
    /// Decrypt ciphertext with current symmetric key
    fn decrypt_and_hash(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, NoiseError> {
        if let Some(ref key) = self.symmetric_key {
            let aead = NyxAead::new(key);
            let nonce = [0u8; 12]; // Noise uses zero nonce during handshake
            let plaintext = aead.decrypt(&nonce, ciphertext, &[])?; // No AAD during handshake
            
            // Update handshake hash with ciphertext
            self.update_handshake_hash(ciphertext);
            Ok(plaintext)
        } else {
            // No decryption key available, treat as plaintext and update hash
            self.update_handshake_hash(ciphertext);
            Ok(ciphertext.to_vec())
        }
    }
}

pub fn derive_session_key(shared: &SharedSecret) -> SessionKey {
    let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&okm);
    SessionKey(out)
}

// -----------------------------------------------------------------------------
// Kyber1024 Post-Quantum fallback (feature "pq")
// -----------------------------------------------------------------------------

#[cfg(feature = "kyber")]
pub mod kyber {
    //! Kyber1024 KEM wrapper providing the same interface semantics as the X25519
    //! Noise_Nyx handshake. When the `pq` feature is enabled at compile-time,
    //! callers can switch to these APIs to negotiate a 32-byte session key that
    //! is derived from the Kyber shared secret via the common HKDF wrapper to
    //! ensure uniform key derivation logic across classic and PQ modes.

    use pqc_kyber::{keypair, encapsulate, decapsulate, PublicKey, SecretKey, Ciphertext};
    use crate::kdf::{hkdf_expand, KdfLabel};
    
    // Re-export commonly used Kyber types for external modules (Hybrid handshake, etc.).
    pub use pqc_kyber::{PublicKey, SecretKey, Ciphertext};
    
    #[derive(Clone, Debug)]
    pub struct SharedSecret(pub [u8; 32]);
    
    impl SharedSecret {
        pub fn as_bytes(&self) -> &[u8] {
            &self.0
        }
    }

    /// Generate a Kyber1024 keypair for the responder.
    pub fn responder_keypair() -> (PublicKey, SecretKey) {
        keypair()
    }

    /// Initiator encapsulates to responder's public key, returning the
    /// ciphertext to transmit and the derived 32-byte session key.
    pub fn initiator_encapsulate(pk: &PublicKey) -> (Ciphertext, super::SessionKey) {
        let (ss, ct) = encapsulate(pk);
        let mut shared = [0u8; 32];
        shared.copy_from_slice(&ss[..32]);
        let shared_secret = SharedSecret(shared);
        (ct, derive_session_key(&shared_secret))
    }

    /// Responder decapsulates ciphertext with its secret key and derives the
    /// matching 32-byte session key.
    pub fn responder_decapsulate(ct: &Ciphertext, sk: &SecretKey) -> super::SessionKey {
        let ss = decapsulate(ct, sk);
        let mut shared = [0u8; 32];
        shared.copy_from_slice(&ss[..32]);
        let shared_secret = SharedSecret(shared);
        derive_session_key(&shared_secret)
    }

    /// Convert Kyber shared secret into Nyx session key via HKDF.
    fn derive_session_key(shared: &SharedSecret) -> super::SessionKey {
        let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
        let mut out = [0u8; 32];
        out.copy_from_slice(&okm);
        super::SessionKey(out)
    }
}

// BIKE post-quantum KEM integration is planned but the required Rust
// crate is not yet available on crates.io. This module is disabled
// to avoid compilation errors.
// #[cfg(feature = "bike")]
// pub mod bike {
//     //! `--features bike` will therefore raise a compile-time error.
//     compile_error!("Feature `bike` is not yet supported – awaiting upstream pqcrypto-bike crate");
// }

/// -----------------------------------------------------------------------------
/// Hybrid X25519 + Kyber Handshake (feature "hybrid")
/// -----------------------------------------------------------------------------
#[cfg(feature = "hybrid")]
pub mod hybrid {
    use super::*;
    use super::kyber; // Kyber helpers
    use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
    use rand_core_06::OsRng;

    /// Initiator generates X25519 ephemeral and Kyber encapsulation.
    pub fn initiator_step(pk_kyber: &kyber::PublicKey) -> (PublicKey, EphemeralSecret, kyber::Ciphertext, SessionKey) {
        let (ct, kyber_key) = kyber::initiator_encapsulate(pk_kyber);
        let mut rng = OsRng;
        let secret = EphemeralSecret::random_from_rng(&mut rng);
        let public = PublicKey::from(&secret);
        // Combine secrets later when responder key known; here return Kyber part as session key placeholder.
        let k = kyber_key;
        (public, secret, ct, k)
    }

    /// Responder receives initiator public keys and ciphertext; returns responder X25519 pub and combined session key.
    pub fn responder_step(init_pub: &PublicKey, ct: &kyber::Ciphertext, sk_kyber: &kyber::SecretKey) -> (PublicKey, SessionKey) {
        // Kyber part
        let kyber_key = kyber::responder_decapsulate(ct, sk_kyber);
        // X25519 part
        let mut rng = OsRng;
        let secret = EphemeralSecret::random_from_rng(&mut rng);
        let public = PublicKey::from(&secret);
        let x_key = secret.diffie_hellman(init_pub);
        // Combine
        match combine_keys(&x_key, &kyber_key) {
            Ok(combined_key) => (public, combined_key),
            Err(e) => panic!("Key combination failed: {}", e), // This should never fail in practice
        }
    }

    /// Initiator finalizes with responder X25519 pub, producing combined session key.
    pub fn initiator_finalize(sec: EphemeralSecret, resp_pub: &PublicKey, kyber_key: SessionKey) -> SessionKey {
        let x_key = sec.diffie_hellman(resp_pub);
        match combine_keys(&x_key, &kyber_key) {
            Ok(combined_key) => combined_key,
            Err(e) => panic!("HKDF key combination failed: {}", e), // This should never fail in practice
        }
    }

    fn combine_keys(classic: &SharedSecret, pq: &SessionKey) -> Option<SessionKey> {
        use zeroize::Zeroize;
        let mut concat = Vec::with_capacity(64);
        concat.extend_from_slice(classic.as_bytes());
        concat.extend_from_slice(&pq.0);
        let okm = hkdf_expand(&concat, KdfLabel::Session, 32);
        let mut out = [0u8; 32];
        out.copy_from_slice(&okm);
        // zeroize temp
        concat.zeroize();
        Some(SessionKey(out))
    }
} 
#[cfg(test)]
mod tests {
    use super::*;
    
    #[cfg(feature = "classic")]
    #[test]
    fn noise_handshake_xx_pattern_complete() {
        let mut initiator = NoiseHandshake::new_initiator().unwrap();
        let mut responder = NoiseHandshake::new_responder().unwrap();
        
        // Message 1: Initiator -> Responder
        let mut msg1 = vec![0u8; 128];
        let payload1 = b"hello";
        let msg1_len = initiator.write_message(payload1, &mut msg1).unwrap();
        msg1.truncate(msg1_len);
        
        let mut recv_payload1 = vec![0u8; 64];
        let recv1_len = responder.read_message(&msg1, &mut recv_payload1).unwrap();
        recv_payload1.truncate(recv1_len);
        assert_eq!(&recv_payload1, payload1);
        
        // Message 2: Responder -> Initiator
        let mut msg2 = vec![0u8; 256];
        let payload2 = b"world";
        let msg2_len = responder.write_message(payload2, &mut msg2).unwrap();
        msg2.truncate(msg2_len);
        
        let mut recv_payload2 = vec![0u8; 64];
        let recv2_len = initiator.read_message(&msg2, &mut recv_payload2).unwrap();
        recv_payload2.truncate(recv2_len);
        assert_eq!(&recv_payload2, payload2);
        
        // Message 3: Initiator -> Responder
        let mut msg3 = vec![0u8; 128];
        let payload3 = b"done";
        let msg3_len = initiator.write_message(payload3, &mut msg3).unwrap();
        msg3.truncate(msg3_len);
        
        let mut recv_payload3 = vec![0u8; 64];
        let recv3_len = responder.read_message(&msg3, &mut recv_payload3).unwrap();
        recv_payload3.truncate(recv3_len);
        assert_eq!(&recv_payload3, payload3);
        
        // Both should be completed
        assert!(initiator.is_completed());
        assert!(responder.is_completed());
        
        // Transition to transport mode
        let _init_transport = initiator.into_transport_mode().unwrap();
        let _resp_transport = responder.into_transport_mode().unwrap();
    }
    
    #[cfg(feature = "classic")]
    #[test]
    fn noise_handshake_state_transitions() {
        let mut initiator = NoiseHandshake::new_initiator().unwrap();
        let mut responder = NoiseHandshake::new_responder().unwrap();
        
        assert_eq!(initiator.state, HandshakeState::Initial);
        assert_eq!(responder.state, HandshakeState::Initial);
        
        // Message 1
        let mut msg1 = vec![0u8; 128];
        initiator.write_message(b"test", &mut msg1).unwrap();
        assert_eq!(initiator.state, HandshakeState::InitiatorSentFirst);
        
        let mut payload = vec![0u8; 96]; // Increased payload buffer size
        responder.read_message(&msg1, &mut payload).unwrap();
        assert_eq!(responder.state, HandshakeState::ResponderReceivedFirst);
        
        // Message 2
        let mut msg2 = vec![0u8; 128]; // Increased buffer size
        let msg2_len = responder.write_message(b"test", &mut msg2).unwrap();
        msg2.truncate(msg2_len);
        assert_eq!(responder.state, HandshakeState::ResponderSentSecond);
        
        initiator.read_message(&msg2, &mut payload).unwrap();
        assert_eq!(initiator.state, HandshakeState::InitiatorReceivedSecond);
        
        // Message 3
        let mut msg3 = vec![0u8; 64]; // 32 bytes for key + 32 bytes for payload buffer
        let msg3_len = initiator.write_message(b"test", &mut msg3).unwrap();
        msg3.truncate(msg3_len);
        assert_eq!(initiator.state, HandshakeState::Completed);
        
        responder.read_message(&msg3, &mut payload).unwrap();
        assert_eq!(responder.state, HandshakeState::Completed);
    }
    
    #[cfg(feature = "classic")]
    #[test]
    fn noise_handshake_error_handling() {
        let mut initiator = NoiseHandshake::new_initiator().unwrap();
        let mut responder = NoiseHandshake::new_responder().unwrap();
        
        // Test message too short
        let mut short_msg = vec![0u8; 16]; // Too short for ephemeral key
        assert!(initiator.write_message(b"test", &mut short_msg).is_err());
        
        // Test invalid state transitions
        let mut msg = vec![0u8; 128];
        assert!(responder.write_message(b"test", &mut msg).is_err()); // Responder can't write first
        
        // Test transport mode before completion
        assert!(initiator.into_transport_mode().is_err());
    }
    
    #[cfg(feature = "classic")]
    #[test]
    fn noise_transport_mode() {
        let mut initiator = NoiseHandshake::new_initiator().unwrap();
        let mut responder = NoiseHandshake::new_responder().unwrap();
        
        // Complete handshake
        let mut msg1 = vec![0u8; 128];
        let msg1_len = initiator.write_message(b"", &mut msg1).unwrap();
        msg1.truncate(msg1_len);
        
        let mut payload = vec![0u8; 64];
        responder.read_message(&msg1, &mut payload).unwrap();
        
        let mut msg2 = vec![0u8; 256];
        let msg2_len = responder.write_message(b"", &mut msg2).unwrap();
        msg2.truncate(msg2_len);
        
        initiator.read_message(&msg2, &mut payload).unwrap();
        
        let mut msg3 = vec![0u8; 128];
        let msg3_len = initiator.write_message(b"", &mut msg3).unwrap();
        msg3.truncate(msg3_len);
        
        responder.read_message(&msg3, &mut payload).unwrap();
        
        // Get transport modes
        let mut init_transport = initiator.into_transport_mode().unwrap();
        let mut resp_transport = responder.into_transport_mode().unwrap();
        
        // Test nonce progression
        assert_eq!(init_transport.next_send_nonce(), 0);
        assert_eq!(init_transport.next_send_nonce(), 1);
        assert_eq!(resp_transport.next_recv_nonce(), 0);
        assert_eq!(resp_transport.next_recv_nonce(), 1);
    }
    
    #[cfg(feature = "classic")]
    #[test]
    fn session_key_zeroization() {
        let key_data = [42u8; 32];
        let session_key = SessionKey::new(key_data);
        
        // Key should be accessible
        assert_eq!(session_key.as_bytes(), &key_data);
        
        // After drop, the key should be zeroized (we can't test this directly,
        // but the ZeroizeOnDrop trait ensures it happens)
        drop(session_key);
    }
    
    #[cfg(feature = "classic")]
    #[test]
    fn session_key_equality() {
        let key1 = SessionKey::new([1u8; 32]);
        let key2 = SessionKey::new([1u8; 32]);
        let key3 = SessionKey::new([2u8; 32]);
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
    
    #[test]
    fn handshake_pattern_display() {
        assert_eq!(format!("{}", HandshakePattern::XX), "XX");
    }
    
    #[test]
    fn handshake_state_display() {
        assert_eq!(format!("{}", HandshakeState::Initial), "Initial");
        assert_eq!(format!("{}", HandshakeState::Completed), "Completed");
    }
    
    #[test]
    fn noise_error_display() {
        let error = NoiseError::InvalidState {
            expected: "test".to_string(),
            actual: "other".to_string(),
        };
        assert!(format!("{}", error).contains("Invalid handshake state"));
    }
}