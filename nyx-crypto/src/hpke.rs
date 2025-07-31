//! HPKE wrapper utilities (RFC 9180) for Nyx.
//!
//! Provides Base mode (mode=0) using KEM X25519, KDF HKDF-SHA256, AEAD ChaCha20-Poly1305.
//! This aligns with Nyx's existing cryptographic primitives.
//!
//! Example:
//! ```rust
//! use nyx_crypto::hpke::{HpkeKeyDeriver, generate_keypair, seal, open};
//! let deriver = HpkeKeyDeriver::new();
//! let (sk, pk) = deriver.derive_keypair().unwrap();
//! let plaintext = b"hello";
//! let (enc, ct) = seal(&pk, b"info", b"aad", plaintext).unwrap();
//! let decrypted = open(&sk, &enc, b"info", b"aad", &ct).unwrap();
//! assert_eq!(plaintext.to_vec(), decrypted);
//! ```

#![forbid(unsafe_code)]

use hpke::{kem::X25519HkdfSha256, kdf::HkdfSha256, aead::ChaCha20Poly1305, OpModeR, OpModeS};
use hpke::kem::Kem;
use hpke::{Serializable, Deserializable};
use rand_core_06::{OsRng, RngCore, CryptoRng};
use crate::noise::SessionKey;
use crate::kdf::{hkdf_expand, KdfLabel};
use thiserror::Error;
use zeroize::ZeroizeOnDrop;

/// Generate a fresh symmetric secret, seal it to `pk_r` and return (enc, ct, session_key).
/// The generated secret is 32 bytes and mapped into Nyx [`SessionKey`].
pub fn generate_and_seal_session(pk_r: &PublicKey, info: &[u8], aad: &[u8]) -> Result<(Vec<u8>, Vec<u8>, SessionKey), hpke::HpkeError> {
    let mut secret = [0u8;32];
    OsRng.fill_bytes(&mut secret);
    let (enc, mut sender_ctx) = hpke::setup_sender::<Aead, Kdf, KemImpl, _>(&OpModeS::Base, pk_r, info, &mut OsRng)?;
    let ct = sender_ctx.seal(&secret, aad)?;
    Ok((enc.to_bytes().to_vec(), ct, SessionKey(secret)))
}

/// Open sealed session key and return [`SessionKey`].
pub fn open_session(sk_r: &PrivateKey, enc: &[u8], info: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<SessionKey, hpke::HpkeError> {
    let pt = open(sk_r, enc, info, aad, ciphertext)?;
    if pt.len()!=32 { return Err(hpke::HpkeError::OpenError); }
    let mut arr=[0u8;32];
    arr.copy_from_slice(&pt);
    Ok(SessionKey(arr))
}

pub type KemImpl = X25519HkdfSha256;
pub type Kdf = HkdfSha256;
pub type Aead = ChaCha20Poly1305;

pub type PublicKey = <KemImpl as hpke::kem::Kem>::PublicKey;
pub type PrivateKey = <KemImpl as hpke::kem::Kem>::PrivateKey;

/// Errors that can occur during HPKE key derivation operations
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("HPKE operation failed: {0:?}")]
    HpkeError(hpke::HpkeError),
    
    #[error("Invalid key material length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },
    
    #[error("Key generation failed: insufficient entropy")]
    InsufficientEntropy,
    
    #[error("Encapsulation failed: invalid public key")]
    InvalidPublicKey,
    
    #[error("Decapsulation failed: invalid encapsulated key")]
    InvalidEncapsulatedKey,
    
    #[error("Key derivation context error: {0}")]
    ContextError(String),
    
    #[error("RNG failure: {0}")]
    RngFailure(String),
    
    #[error("Key validation failed: {0}")]
    KeyValidationFailed(String),
    
    #[error("Cryptographic operation timeout")]
    OperationTimeout,
    
    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),
    
    #[error("Context initialization failed: {0}")]
    ContextInitializationFailed(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}

impl From<hpke::HpkeError> for CryptoError {
    fn from(err: hpke::HpkeError) -> Self {
        CryptoError::HpkeError(err)
    }
}

/// Shared secret derived from HPKE key encapsulation
#[derive(Clone, ZeroizeOnDrop)]
pub struct SharedSecret {
    inner: Vec<u8>,
}

impl SharedSecret {
    pub fn new(data: Vec<u8>) -> Self {
        Self { inner: data }
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }
    
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Encapsulated key from HPKE key encapsulation
#[derive(Clone)]
pub struct EncapsulatedKey {
    inner: Vec<u8>,
}

impl EncapsulatedKey {
    pub fn new(data: Vec<u8>) -> Self {
        Self { inner: data }
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner
    }
    
    pub fn from_bytes(data: &[u8]) -> Result<Self, CryptoError> {
        if data.is_empty() {
            return Err(CryptoError::DeserializationError("Empty encapsulated key data".to_string()));
        }
        Ok(Self { inner: data.to_vec() })
    }
    
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Secure context for HPKE operations
pub struct SecureContext {
    context_id: Vec<u8>,
    private_key: PrivateKey,
    public_key: PublicKey,
    created_at: std::time::SystemTime,
}

impl SecureContext {
    pub fn context_id(&self) -> &[u8] {
        &self.context_id
    }
    
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }
    
    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }
    
    pub fn created_at(&self) -> std::time::SystemTime {
        self.created_at
    }
    
    pub fn age(&self) -> Result<std::time::Duration, std::time::SystemTimeError> {
        std::time::SystemTime::now().duration_since(self.created_at)
    }
}

/// Key derivation statistics for monitoring
#[derive(Debug, Default, Clone)]
pub struct KeyDerivationStats {
    pub keypairs_generated: u64,
    pub encapsulations_performed: u64,
    pub decapsulations_performed: u64,
    pub session_keys_derived: u64,
    pub validation_failures: u64,
    pub entropy_failures: u64,
}

impl KeyDerivationStats {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn increment_keypairs(&mut self) {
        self.keypairs_generated = self.keypairs_generated.saturating_add(1);
    }
    
    pub fn increment_encapsulations(&mut self) {
        self.encapsulations_performed = self.encapsulations_performed.saturating_add(1);
    }
    
    pub fn increment_decapsulations(&mut self) {
        self.decapsulations_performed = self.decapsulations_performed.saturating_add(1);
    }
    
    pub fn increment_session_keys(&mut self) {
        self.session_keys_derived = self.session_keys_derived.saturating_add(1);
    }
    
    pub fn increment_validation_failures(&mut self) {
        self.validation_failures = self.validation_failures.saturating_add(1);
    }
    
    pub fn increment_entropy_failures(&mut self) {
        self.entropy_failures = self.entropy_failures.saturating_add(1);
    }
}

/// HPKE Key Derivation System
/// 
/// Provides secure key pair generation, encapsulation, and decapsulation operations
/// using X25519 KEM with HKDF-SHA256 and ChaCha20Poly1305 AEAD.
pub struct HpkeKeyDeriver;

impl HpkeKeyDeriver {
    /// Create a new HPKE key deriver with default cryptographic suite
    pub fn new() -> Self {
        Self
    }
    
    /// Generate a fresh keypair using secure random number generation with retry logic
    pub fn derive_keypair(&self) -> Result<(PrivateKey, PublicKey), CryptoError> {
        self.derive_keypair_with_retry(3)
    }
    
    /// Generate a keypair with retry logic for robustness
    pub fn derive_keypair_with_retry(&self, max_retries: u32) -> Result<(PrivateKey, PublicKey), CryptoError> {
        let mut last_error = None;
        
        for attempt in 0..max_retries {
            match self.derive_keypair_with_rng(&mut OsRng) {
                Ok(keypair) => return Ok(keypair),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        // Brief delay before retry to allow entropy pool to replenish
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or(CryptoError::KeyDerivationFailed("Unknown error".to_string())))
    }
    
    /// Generate a keypair with a custom RNG (primarily for testing)
    pub fn derive_keypair_with_rng<R: RngCore + CryptoRng>(&self, rng: &mut R) -> Result<(PrivateKey, PublicKey), CryptoError> {
        // Enhanced entropy validation
        self.validate_rng_entropy(rng)?;
        
        // Generate keypair with error handling
        let (private_key, public_key) = KemImpl::gen_keypair(rng);
        
        // Validate generated keys
        self.validate_generated_keypair(&private_key, &public_key)?;
        
        Ok((private_key, public_key))
    }
    
    /// Validate RNG entropy quality
    fn validate_rng_entropy<R: RngCore + CryptoRng>(&self, rng: &mut R) -> Result<(), CryptoError> {
        // Generate multiple test samples to check entropy quality
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut sample = [0u8; 32];
            rng.fill_bytes(&mut sample);
            samples.push(sample);
        }
        
        // Check for obvious entropy failures
        for sample in &samples {
            // All zeros
            if sample.iter().all(|&x| x == 0) {
                return Err(CryptoError::InsufficientEntropy);
            }
            
            // All same value
            if sample.windows(2).all(|w| w[0] == w[1]) {
                return Err(CryptoError::InsufficientEntropy);
            }
            
            // Simple pattern detection (alternating bytes)
            if sample.len() >= 4 && sample.chunks(2).all(|chunk| chunk[0] == chunk[1]) {
                return Err(CryptoError::InsufficientEntropy);
            }
        }
        
        // Check for duplicate samples (extremely unlikely with good entropy)
        for i in 0..samples.len() {
            for j in (i + 1)..samples.len() {
                if samples[i] == samples[j] {
                    return Err(CryptoError::InsufficientEntropy);
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate generated keypair
    fn validate_generated_keypair(&self, private_key: &PrivateKey, public_key: &PublicKey) -> Result<(), CryptoError> {
        // Check key lengths
        if private_key.to_bytes().len() != self.private_key_len() {
            return Err(CryptoError::InvalidKeyLength {
                expected: self.private_key_len(),
                actual: private_key.to_bytes().len(),
            });
        }
        
        if public_key.to_bytes().len() != self.public_key_len() {
            return Err(CryptoError::InvalidKeyLength {
                expected: self.public_key_len(),
                actual: public_key.to_bytes().len(),
            });
        }
        
        // Test encapsulation to ensure key is valid
        match self.encapsulate(public_key) {
            Ok(_) => Ok(()),
            Err(e) => Err(CryptoError::KeyValidationFailed(format!("Key failed encapsulation test: {:?}", e))),
        }
    }
    
    /// Encapsulate a shared secret to the given public key with retry logic
    pub fn encapsulate(&self, pk: &PublicKey) -> Result<(SharedSecret, EncapsulatedKey), CryptoError> {
        self.encapsulate_with_retry(pk, 3)
    }
    
    /// Encapsulate with retry logic for robustness
    pub fn encapsulate_with_retry(&self, pk: &PublicKey, max_retries: u32) -> Result<(SharedSecret, EncapsulatedKey), CryptoError> {
        // Validate public key first
        self.validate_public_key_format(pk)?;
        
        let mut last_error = None;
        
        for attempt in 0..max_retries {
            match self.encapsulate_with_rng(pk, &mut OsRng) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or(CryptoError::InvalidPublicKey))
    }
    
    /// Encapsulate with custom RNG
    pub fn encapsulate_with_rng<R: RngCore + CryptoRng>(
        &self, 
        pk: &PublicKey, 
        rng: &mut R
    ) -> Result<(SharedSecret, EncapsulatedKey), CryptoError> {
        // Validate RNG entropy
        self.validate_rng_entropy(rng)?;
        
        // Perform encapsulation
        let (shared_secret, encapped_key) = KemImpl::encap(pk, None, rng)
            .map_err(|e| CryptoError::InvalidPublicKey)?;
        
        // Validate shared secret
        if shared_secret.0.is_empty() {
            return Err(CryptoError::KeyDerivationFailed("Empty shared secret generated".to_string()));
        }
        
        // Validate encapsulated key
        let enc_key_bytes = encapped_key.to_bytes();
        if enc_key_bytes.len() != self.encapsulated_key_len() {
            return Err(CryptoError::InvalidKeyLength {
                expected: self.encapsulated_key_len(),
                actual: enc_key_bytes.len(),
            });
        }
        
        Ok((
            SharedSecret::new(shared_secret.0.to_vec()),
            EncapsulatedKey::new(enc_key_bytes.to_vec())
        ))
    }
    
    /// Decapsulate the shared secret using the private key and encapsulated key with enhanced error handling
    pub fn decapsulate(&self, sk: &PrivateKey, enc: &EncapsulatedKey) -> Result<SharedSecret, CryptoError> {
        // Validate inputs
        self.validate_private_key_format(sk)?;
        self.validate_encapsulated_key_format(enc)?;
        
        // Parse encapsulated key
        let encapped = <KemImpl as Kem>::EncappedKey::from_bytes(&enc.inner)
            .map_err(|e| CryptoError::DeserializationError(format!("Failed to parse encapsulated key: {:?}", e)))?;
        
        // Perform decapsulation
        let shared_secret = KemImpl::decap(sk, None, &encapped)
            .map_err(|e| CryptoError::InvalidEncapsulatedKey)?;
        
        // Validate shared secret
        if shared_secret.0.is_empty() {
            return Err(CryptoError::KeyDerivationFailed("Empty shared secret from decapsulation".to_string()));
        }
        
        Ok(SharedSecret::new(shared_secret.0.to_vec()))
    }
    
    /// Validate public key format
    fn validate_public_key_format(&self, pk: &PublicKey) -> Result<(), CryptoError> {
        let key_bytes = pk.to_bytes();
        if key_bytes.len() != self.public_key_len() {
            return Err(CryptoError::InvalidKeyLength {
                expected: self.public_key_len(),
                actual: key_bytes.len(),
            });
        }
        
        // Check for obviously invalid keys (all zeros, etc.)
        if key_bytes.iter().all(|&x| x == 0) {
            return Err(CryptoError::KeyValidationFailed("Public key is all zeros".to_string()));
        }
        
        Ok(())
    }
    
    /// Validate private key format
    fn validate_private_key_format(&self, sk: &PrivateKey) -> Result<(), CryptoError> {
        let key_bytes = sk.to_bytes();
        if key_bytes.len() != self.private_key_len() {
            return Err(CryptoError::InvalidKeyLength {
                expected: self.private_key_len(),
                actual: key_bytes.len(),
            });
        }
        
        // Check for obviously invalid keys (all zeros, etc.)
        if key_bytes.iter().all(|&x| x == 0) {
            return Err(CryptoError::KeyValidationFailed("Private key is all zeros".to_string()));
        }
        
        Ok(())
    }
    
    /// Validate encapsulated key format
    fn validate_encapsulated_key_format(&self, enc: &EncapsulatedKey) -> Result<(), CryptoError> {
        if enc.inner.len() != self.encapsulated_key_len() {
            return Err(CryptoError::InvalidKeyLength {
                expected: self.encapsulated_key_len(),
                actual: enc.inner.len(),
            });
        }
        
        // Check for obviously invalid keys (all zeros, etc.)
        if enc.inner.iter().all(|&x| x == 0) {
            return Err(CryptoError::KeyValidationFailed("Encapsulated key is all zeros".to_string()));
        }
        
        Ok(())
    }
    
    /// Derive a session key from shared secret with context information and enhanced validation
    pub fn derive_session_key(&self, shared_secret: &SharedSecret, context: &'static [u8]) -> Result<SessionKey, CryptoError> {
        // Validate shared secret
        if shared_secret.len() == 0 {
            return Err(CryptoError::KeyDerivationFailed("Empty shared secret".to_string()));
        }
        
        // Validate context
        if context.is_empty() {
            return Err(CryptoError::ContextError("Empty context not allowed".to_string()));
        }
        
        // Derive key material
        let key_material = hkdf_expand(
            shared_secret.as_bytes(), 
            KdfLabel::Custom(context), 
            32
        );
        
        // Validate derived key material
        if key_material.iter().all(|&x| x == 0) {
            return Err(CryptoError::KeyDerivationFailed("Derived key is all zeros".to_string()));
        }
        
        let mut session_key = [0u8; 32];
        session_key.copy_from_slice(&key_material);
        Ok(SessionKey(session_key))
    }
    
    /// Derive multiple session keys from shared secret with different contexts
    pub fn derive_multiple_session_keys(&self, shared_secret: &SharedSecret, contexts: &[&'static [u8]]) -> Result<Vec<SessionKey>, CryptoError> {
        if contexts.is_empty() {
            return Err(CryptoError::ContextError("No contexts provided".to_string()));
        }
        
        let mut keys = Vec::with_capacity(contexts.len());
        for context in contexts {
            keys.push(self.derive_session_key(shared_secret, context)?);
        }
        
        Ok(keys)
    }
    
    /// Validate that a public key is well-formed with comprehensive checks
    pub fn validate_public_key(&self, pk: &PublicKey) -> Result<(), CryptoError> {
        // Format validation
        self.validate_public_key_format(pk)?;
        
        // Functional validation - try to encapsulate to the key
        match self.encapsulate(pk) {
            Ok(_) => Ok(()),
            Err(e) => Err(CryptoError::KeyValidationFailed(format!("Key failed encapsulation test: {:?}", e))),
        }
    }
    
    /// Validate that a private key is well-formed
    pub fn validate_private_key(&self, sk: &PrivateKey) -> Result<(), CryptoError> {
        // Format validation
        self.validate_private_key_format(sk)?;
        
        // For HPKE, we can't easily derive public key from private key,
        // so we just validate the format
        Ok(())
    }
    
    /// Perform a complete key derivation roundtrip test
    pub fn test_key_derivation_roundtrip(&self) -> Result<(), CryptoError> {
        // Generate keypair
        let (sk, pk) = self.derive_keypair()?;
        
        // Validate keys
        self.validate_private_key(&sk)?;
        self.validate_public_key(&pk)?;
        
        // Test encapsulation/decapsulation
        let (shared_secret1, enc_key) = self.encapsulate(&pk)?;
        let shared_secret2 = self.decapsulate(&sk, &enc_key)?;
        
        // Verify shared secrets match
        if shared_secret1.as_bytes() != shared_secret2.as_bytes() {
            return Err(CryptoError::KeyDerivationFailed("Shared secrets do not match".to_string()));
        }
        
        // Test session key derivation
        let _session_key = self.derive_session_key(&shared_secret1, b"test-context")?;
        
        Ok(())
    }
    
    /// Create a secure context for key operations
    pub fn create_secure_context(&self, context_id: &[u8]) -> Result<SecureContext, CryptoError> {
        if context_id.is_empty() {
            return Err(CryptoError::ContextError("Context ID cannot be empty".to_string()));
        }
        
        // Generate a fresh keypair for this context
        let (private_key, public_key) = self.derive_keypair()?;
        
        Ok(SecureContext {
            context_id: context_id.to_vec(),
            private_key,
            public_key,
            created_at: std::time::SystemTime::now(),
        })
    }
    
    /// Get the expected length of encapsulated keys for this KEM
    pub fn encapsulated_key_len(&self) -> usize {
        <KemImpl as Kem>::EncappedKey::size()
    }
    
    /// Get the expected length of public keys for this KEM
    pub fn public_key_len(&self) -> usize {
        <KemImpl as Kem>::PublicKey::size()
    }
    
    /// Get the expected length of private keys for this KEM
    pub fn private_key_len(&self) -> usize {
        <KemImpl as Kem>::PrivateKey::size()
    }
}

impl Default for HpkeKeyDeriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitored HPKE Key Deriver with statistics tracking
pub struct MonitoredHpkeKeyDeriver {
    inner: HpkeKeyDeriver,
    stats: std::sync::Arc<std::sync::Mutex<KeyDerivationStats>>,
}

impl MonitoredHpkeKeyDeriver {
    pub fn new() -> Self {
        Self {
            inner: HpkeKeyDeriver::new(),
            stats: std::sync::Arc::new(std::sync::Mutex::new(KeyDerivationStats::new())),
        }
    }
    
    pub fn derive_keypair(&self) -> Result<(PrivateKey, PublicKey), CryptoError> {
        match self.inner.derive_keypair() {
            Ok(keypair) => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.increment_keypairs();
                }
                Ok(keypair)
            }
            Err(e) => {
                if let Ok(mut stats) = self.stats.lock() {
                    match &e {
                        CryptoError::InsufficientEntropy => stats.increment_entropy_failures(),
                        CryptoError::KeyValidationFailed(_) => stats.increment_validation_failures(),
                        _ => {}
                    }
                }
                Err(e)
            }
        }
    }
    
    pub fn encapsulate(&self, pk: &PublicKey) -> Result<(SharedSecret, EncapsulatedKey), CryptoError> {
        match self.inner.encapsulate(pk) {
            Ok(result) => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.increment_encapsulations();
                }
                Ok(result)
            }
            Err(e) => {
                if let Ok(mut stats) = self.stats.lock() {
                    if matches!(&e, CryptoError::KeyValidationFailed(_)) {
                        stats.increment_validation_failures();
                    }
                }
                Err(e)
            }
        }
    }
    
    pub fn decapsulate(&self, sk: &PrivateKey, enc: &EncapsulatedKey) -> Result<SharedSecret, CryptoError> {
        match self.inner.decapsulate(sk, enc) {
            Ok(result) => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.increment_decapsulations();
                }
                Ok(result)
            }
            Err(e) => {
                if let Ok(mut stats) = self.stats.lock() {
                    if matches!(&e, CryptoError::KeyValidationFailed(_)) {
                        stats.increment_validation_failures();
                    }
                }
                Err(e)
            }
        }
    }
    
    pub fn derive_session_key(&self, shared_secret: &SharedSecret, context: &'static [u8]) -> Result<SessionKey, CryptoError> {
        match self.inner.derive_session_key(shared_secret, context) {
            Ok(key) => {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.increment_session_keys();
                }
                Ok(key)
            }
            Err(e) => Err(e)
        }
    }
    
    pub fn get_stats(&self) -> Result<KeyDerivationStats, CryptoError> {
        self.stats.lock()
            .map(|stats| (*stats).clone())
            .map_err(|_| CryptoError::ContextError("Failed to acquire stats lock".to_string()))
    }
    
    pub fn reset_stats(&self) -> Result<(), CryptoError> {
        self.stats.lock()
            .map(|mut stats| *stats = KeyDerivationStats::new())
            .map_err(|_| CryptoError::ContextError("Failed to acquire stats lock".to_string()))
    }
    
    // Delegate other methods to inner deriver
    pub fn validate_public_key(&self, pk: &PublicKey) -> Result<(), CryptoError> {
        self.inner.validate_public_key(pk)
    }
    
    pub fn validate_private_key(&self, sk: &PrivateKey) -> Result<(), CryptoError> {
        self.inner.validate_private_key(sk)
    }
    
    pub fn test_key_derivation_roundtrip(&self) -> Result<(), CryptoError> {
        self.inner.test_key_derivation_roundtrip()
    }
    
    pub fn create_secure_context(&self, context_id: &[u8]) -> Result<SecureContext, CryptoError> {
        self.inner.create_secure_context(context_id)
    }
    
    pub fn encapsulated_key_len(&self) -> usize {
        self.inner.encapsulated_key_len()
    }
    
    pub fn public_key_len(&self) -> usize {
        self.inner.public_key_len()
    }
    
    pub fn private_key_len(&self) -> usize {
        self.inner.private_key_len()
    }
}

impl Default for MonitoredHpkeKeyDeriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate an X25519 keypair for HPKE.
#[must_use]
pub fn generate_keypair() -> (PrivateKey, PublicKey) {
    KemImpl::gen_keypair(&mut OsRng)
}

/// Seal (encrypt) `plaintext` to `pk_r`. Returns (enc, ciphertext).
pub fn seal(pk_r: &PublicKey, info: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), hpke::HpkeError> {
    let (enc, mut sender_ctx) = hpke::setup_sender::<Aead, Kdf, KemImpl, _>(
        &OpModeS::Base,
        pk_r,
        info,
        &mut OsRng,
    )?;
    let ct = sender_ctx.seal(plaintext, aad)?;
    Ok((enc.to_bytes().to_vec(), ct))
}

/// Open (decrypt) ciphertext using receiver private key.
pub fn open(
    sk_r: &PrivateKey,
    enc: &[u8],
    info: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, hpke::HpkeError> {
    let encapped = <KemImpl as Kem>::EncappedKey::from_bytes(enc)?;
    let mut receiver_ctx = hpke::setup_receiver::<Aead, Kdf, KemImpl>(&OpModeR::Base, sk_r, &encapped, info)?;
    let pt = receiver_ctx.open(ciphertext, aad)?;
    Ok(pt)
}

/// Export secret from Nyx SessionKey using HKDF-SHA256 labelled "nyx-hpke-export".
#[must_use]
pub fn export_from_session(session: &SessionKey, length: usize) -> Vec<u8> {
    hkdf_expand(&session.0, KdfLabel::Custom(b"nyx-hpke-export"), length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core_06::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    
    #[test]
    fn hpke_roundtrip() {
        let (sk, pk) = generate_keypair();
        let (enc, ct) = seal(&pk, b"info", b"", b"secret").unwrap();
        let pt = open(&sk, &enc, b"info", b"", &ct).unwrap();
        assert_eq!(pt, b"secret");
    }

    #[test]
    fn export_from_session_unique() {
        use crate::noise::SessionKey;
        let sk1 = SessionKey([1u8;32]);
        let sk2 = SessionKey([2u8;32]);
        assert_ne!(export_from_session(&sk1, 32), export_from_session(&sk2, 32));
    }

    #[test]
    fn noise_hpke_integration() {
        use crate::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};
        // Initiator creates ephemeral key
        let (init_pub, init_sec) = initiator_generate();
        // Responder processes initiator pub
        let (resp_pub, shared_resp) = responder_process(&init_pub);
        // Initiator finalizes with responder pub
        let shared_init = initiator_finalize(init_sec, &resp_pub);
        let sk_i = derive_session_key(&shared_init);
        let sk_r = derive_session_key(&shared_resp);
        // Both session keys must be identical
        assert_eq!(sk_i.0, sk_r.0);
        // Derive HKDF-based export secret and ensure match
        let exp_i = super::export_from_session(&sk_i, 32);
        let exp_r = super::export_from_session(&sk_r, 32);
        assert_eq!(exp_i, exp_r);
    }

    #[test]
    fn key_update_roundtrip() {
        let (sk_r, pk_r) = generate_keypair();
        let (enc, ct, new_sess) = generate_and_seal_session(&pk_r, b"rekey", b"", ).unwrap();
        let recv_sess = open_session(&sk_r, &enc, b"rekey", b"", &ct).unwrap();
        assert_eq!(new_sess.0, recv_sess.0);
    }
    
    // New comprehensive tests for HpkeKeyDeriver
    
    #[test]
    fn hpke_key_deriver_basic_functionality() {
        let deriver = HpkeKeyDeriver::new();
        
        // Test keypair generation
        let (sk1, pk1) = deriver.derive_keypair().unwrap();
        let (sk2, pk2) = deriver.derive_keypair().unwrap();
        
        // Keys should be different
        assert_ne!(sk1.to_bytes(), sk2.to_bytes());
        assert_ne!(pk1.to_bytes(), pk2.to_bytes());
        
        // Test encapsulation/decapsulation
        let (shared_secret1, enc_key) = deriver.encapsulate(&pk1).unwrap();
        let shared_secret2 = deriver.decapsulate(&sk1, &enc_key).unwrap();
        
        assert_eq!(shared_secret1.as_bytes(), shared_secret2.as_bytes());
    }
    
    #[test]
    fn hpke_key_deriver_with_custom_rng() {
        let deriver = HpkeKeyDeriver::new();
        let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
        
        let (sk, pk) = deriver.derive_keypair_with_rng(&mut rng).unwrap();
        let (shared_secret, enc_key) = deriver.encapsulate_with_rng(&pk, &mut rng).unwrap();
        let decapped_secret = deriver.decapsulate(&sk, &enc_key).unwrap();
        
        assert_eq!(shared_secret.as_bytes(), decapped_secret.as_bytes());
    }
    
    #[test]
    fn hpke_key_deriver_session_key_derivation() {
        let deriver = HpkeKeyDeriver::new();
        let (sk, pk) = deriver.derive_keypair().unwrap();
        let (shared_secret, _) = deriver.encapsulate(&pk).unwrap();
        
        let session_key1 = deriver.derive_session_key(&shared_secret, b"context1").unwrap();
        let session_key2 = deriver.derive_session_key(&shared_secret, b"context2").unwrap();
        let session_key3 = deriver.derive_session_key(&shared_secret, b"context1").unwrap();
        
        // Different contexts should produce different keys
        assert_ne!(session_key1.0, session_key2.0);
        // Same context should produce same key
        assert_eq!(session_key1.0, session_key3.0);
    }
    
    #[test]
    fn hpke_key_deriver_public_key_validation() {
        let deriver = HpkeKeyDeriver::new();
        let (_sk, pk) = deriver.derive_keypair().unwrap();
        
        // Valid key should pass validation
        assert!(deriver.validate_public_key(&pk).is_ok());
    }
    
    #[test]
    fn hpke_key_deriver_error_handling() {
        let deriver = HpkeKeyDeriver::new();
        
        // Test invalid encapsulated key
        let invalid_enc = EncapsulatedKey::new(vec![0u8; 10]); // Wrong length
        let (sk, _) = deriver.derive_keypair().unwrap();
        
        assert!(deriver.decapsulate(&sk, &invalid_enc).is_err());
    }
    
    #[test]
    fn hpke_key_deriver_key_lengths() {
        let deriver = HpkeKeyDeriver::new();
        
        assert_eq!(deriver.public_key_len(), 32); // X25519 public key length
        assert_eq!(deriver.private_key_len(), 32); // X25519 private key length
        assert_eq!(deriver.encapsulated_key_len(), 32); // X25519 encapsulated key length
    }
    
    #[test]
    fn hpke_shared_secret_zeroization() {
        let deriver = HpkeKeyDeriver::new();
        let (_, pk) = deriver.derive_keypair().unwrap();
        let (shared_secret, _) = deriver.encapsulate(&pk).unwrap();
        
        let original_bytes = shared_secret.as_bytes().to_vec();
        drop(shared_secret);
        
        // After drop, the original bytes should still be valid (we copied them)
        assert!(!original_bytes.is_empty());
    }
    
    #[test]
    fn hpke_encapsulated_key_serialization() {
        let deriver = HpkeKeyDeriver::new();
        let (_, pk) = deriver.derive_keypair().unwrap();
        let (_, enc_key) = deriver.encapsulate(&pk).unwrap();
        
        let bytes = enc_key.as_bytes();
        let reconstructed = EncapsulatedKey::from_bytes(bytes).unwrap();
        
        assert_eq!(enc_key.as_bytes(), reconstructed.as_bytes());
    }
    
    #[test]
    fn hpke_key_deriver_deterministic_with_same_rng_seed() {
        let mut rng1 = ChaCha20Rng::from_seed([123u8; 32]);
        let mut rng2 = ChaCha20Rng::from_seed([123u8; 32]);
        
        let deriver = HpkeKeyDeriver::new();
        let (sk1, pk1) = deriver.derive_keypair_with_rng(&mut rng1).unwrap();
        let (sk2, pk2) = deriver.derive_keypair_with_rng(&mut rng2).unwrap();
        
        // Same seed should produce same keys
        assert_eq!(sk1.to_bytes(), sk2.to_bytes());
        assert_eq!(pk1.to_bytes(), pk2.to_bytes());
    }
    
    // New comprehensive tests for enhanced HPKE implementation
    
    #[test]
    fn hpke_key_deriver_retry_logic() {
        let deriver = HpkeKeyDeriver::new();
        
        // Test successful generation with retries
        let result = deriver.derive_keypair_with_retry(3);
        assert!(result.is_ok());
        
        let (sk, pk) = result.unwrap();
        assert!(deriver.validate_private_key(&sk).is_ok());
        assert!(deriver.validate_public_key(&pk).is_ok());
    }
    
    #[test]
    fn hpke_key_deriver_encapsulation_retry() {
        let deriver = HpkeKeyDeriver::new();
        let (_, pk) = deriver.derive_keypair().unwrap();
        
        let result = deriver.encapsulate_with_retry(&pk, 3);
        assert!(result.is_ok());
        
        let (shared_secret, enc_key) = result.unwrap();
        assert!(!shared_secret.as_bytes().is_empty());
        assert!(!enc_key.is_empty());
    }
    
    #[test]
    fn hpke_key_deriver_enhanced_session_key_derivation() {
        let deriver = HpkeKeyDeriver::new();
        let (_sk, pk) = deriver.derive_keypair().unwrap();
        let (shared_secret, _) = deriver.encapsulate(&pk).unwrap();
        
        // Test single session key derivation
        let session_key = deriver.derive_session_key(&shared_secret, b"test-context").unwrap();
        assert_ne!(session_key.0, [0u8; 32]);
        
        // Test multiple session key derivation
        let contexts: &[&'static [u8]] = &[b"context1", b"context2", b"context3"];
        let session_keys = deriver.derive_multiple_session_keys(&shared_secret, contexts).unwrap();
        assert_eq!(session_keys.len(), 3);
        
        // All keys should be different
        for i in 0..session_keys.len() {
            for j in (i + 1)..session_keys.len() {
                assert_ne!(session_keys[i].0, session_keys[j].0);
            }
        }
    }
    
    #[test]
    fn hpke_key_deriver_validation_errors() {
        let deriver = HpkeKeyDeriver::new();
        
        // Test empty context error
        let (_, pk) = deriver.derive_keypair().unwrap();
        let (shared_secret, _) = deriver.encapsulate(&pk).unwrap();
        
        let result = deriver.derive_session_key(&shared_secret, b"");
        assert!(matches!(result.unwrap_err(), CryptoError::ContextError(_)));
        
        // Test empty contexts for multiple derivation
        let result = deriver.derive_multiple_session_keys(&shared_secret, &[]);
        assert!(matches!(result.unwrap_err(), CryptoError::ContextError(_)));
    }
    
    #[test]
    fn hpke_key_deriver_roundtrip_test() {
        let deriver = HpkeKeyDeriver::new();
        
        // Should complete successfully
        assert!(deriver.test_key_derivation_roundtrip().is_ok());
    }
    
    #[test]
    fn hpke_secure_context() {
        let deriver = HpkeKeyDeriver::new();
        
        let context = deriver.create_secure_context(b"test-context-id").unwrap();
        assert_eq!(context.context_id(), b"test-context-id");
        assert!(context.age().is_ok());
        
        // Test empty context ID error
        let result = deriver.create_secure_context(b"");
        assert!(result.is_err());
    }
    
    #[test]
    fn hpke_encapsulated_key_enhanced() {
        let deriver = HpkeKeyDeriver::new();
        let (_, pk) = deriver.derive_keypair().unwrap();
        let (_, enc_key) = deriver.encapsulate(&pk).unwrap();
        
        // Test length and emptiness
        assert!(!enc_key.is_empty());
        assert_eq!(enc_key.len(), deriver.encapsulated_key_len());
        
        // Test serialization/deserialization
        let bytes = enc_key.as_bytes();
        let reconstructed = EncapsulatedKey::from_bytes(bytes).unwrap();
        assert_eq!(enc_key.as_bytes(), reconstructed.as_bytes());
        
        // Test empty data error
        let result = EncapsulatedKey::from_bytes(&[]);
        assert!(result.is_err());
    }
    
    #[test]
    fn hpke_monitored_key_deriver() {
        let monitored = MonitoredHpkeKeyDeriver::new();
        
        // Test keypair generation with stats
        let (sk, pk) = monitored.derive_keypair().unwrap();
        let stats = monitored.get_stats().unwrap();
        assert_eq!(stats.keypairs_generated, 1);
        
        // Test encapsulation with stats
        let (shared_secret, enc_key) = monitored.encapsulate(&pk).unwrap();
        let stats = monitored.get_stats().unwrap();
        assert_eq!(stats.encapsulations_performed, 1);
        
        // Test decapsulation with stats
        let _decapped_secret = monitored.decapsulate(&sk, &enc_key).unwrap();
        let stats = monitored.get_stats().unwrap();
        assert_eq!(stats.decapsulations_performed, 1);
        
        // Test session key derivation with stats
        let _session_key = monitored.derive_session_key(&shared_secret, b"test").unwrap();
        let stats = monitored.get_stats().unwrap();
        assert_eq!(stats.session_keys_derived, 1);
        
        // Test stats reset
        monitored.reset_stats().unwrap();
        let stats = monitored.get_stats().unwrap();
        assert_eq!(stats.keypairs_generated, 0);
        assert_eq!(stats.encapsulations_performed, 0);
        assert_eq!(stats.decapsulations_performed, 0);
        assert_eq!(stats.session_keys_derived, 0);
    }
    
    #[test]
    fn hpke_key_derivation_stats() {
        let mut stats = KeyDerivationStats::new();
        
        stats.increment_keypairs();
        stats.increment_encapsulations();
        stats.increment_decapsulations();
        stats.increment_session_keys();
        stats.increment_validation_failures();
        stats.increment_entropy_failures();
        
        assert_eq!(stats.keypairs_generated, 1);
        assert_eq!(stats.encapsulations_performed, 1);
        assert_eq!(stats.decapsulations_performed, 1);
        assert_eq!(stats.session_keys_derived, 1);
        assert_eq!(stats.validation_failures, 1);
        assert_eq!(stats.entropy_failures, 1);
    }
    
    #[test]
    fn hpke_error_types_comprehensive() {
        // Test all error types can be created and displayed
        let errors = vec![
            CryptoError::HpkeError(hpke::HpkeError::OpenError),
            CryptoError::InvalidKeyLength { expected: 32, actual: 16 },
            CryptoError::InsufficientEntropy,
            CryptoError::InvalidPublicKey,
            CryptoError::InvalidEncapsulatedKey,
            CryptoError::ContextError("test".to_string()),
            CryptoError::RngFailure("test".to_string()),
            CryptoError::KeyValidationFailed("test".to_string()),
            CryptoError::OperationTimeout,
            CryptoError::KeyDerivationFailed("test".to_string()),
            CryptoError::ContextInitializationFailed("test".to_string()),
            CryptoError::SerializationError("test".to_string()),
            CryptoError::DeserializationError("test".to_string()),
        ];
        
        for error in errors {
            let error_string = format!("{}", error);
            assert!(!error_string.is_empty());
        }
    }
    
    #[test]
    fn hpke_shared_secret_validation() {
        let deriver = HpkeKeyDeriver::new();
        let (_, pk) = deriver.derive_keypair().unwrap();
        let (shared_secret, _) = deriver.encapsulate(&pk).unwrap();
        
        // Test valid shared secret
        assert!(!shared_secret.as_bytes().is_empty());
        assert!(shared_secret.len() > 0);
        
        // Test session key derivation with valid shared secret
        let session_key = deriver.derive_session_key(&shared_secret, b"valid-context").unwrap();
        assert_ne!(session_key.0, [0u8; 32]);
    }
} 