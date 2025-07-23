#![forbid(unsafe_code)]

//! Session management system for Nyx daemon.
//!
//! This module handles:
//! - Connection ID (CID) generation and management
//! - Session lifecycle (creation, maintenance, cleanup)
//! - Cryptographic context management
//! - Session-based routing and multiplexing
//! - Session statistics and monitoring

use dashmap::DashMap;
use nyx_core::NodeId;
use nyx_crypto::aead::FrameCrypter;
use nyx_stream::StreamState;

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{RwLock, Mutex};
use tokio::time::interval;
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;
use rand::RngCore;

/// 96-bit Connection ID as specified in Nyx Protocol
pub type ConnectionId = [u8; 12];

/// Session configuration
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    pub max_sessions: u32,
    pub session_timeout_secs: u64,
    pub cleanup_interval_secs: u64,
    pub enable_session_persistence: bool,
    pub max_streams_per_session: u32,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            max_sessions: 10000,
            session_timeout_secs: 3600, // 1 hour
            cleanup_interval_secs: 300,  // 5 minutes
            enable_session_persistence: false,
            max_streams_per_session: 100,
        }
    }
}

/// Session information
#[derive(Debug, Clone)]
pub struct Session {
    pub cid: ConnectionId,
    pub peer_node_id: Option<NodeId>,
    pub state: SessionState,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub stream_count: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub crypter: Option<Arc<FrameCrypter>>,
    pub metadata: HashMap<String, String>,
}

/// Session state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Initializing,
    Handshaking,
    Active,
    Closing,
    Closed,
}

/// Session statistics
#[derive(Debug, Clone, Default)]
pub struct SessionStatistics {
    pub total_sessions_created: u64,
    pub active_sessions: u32,
    pub expired_sessions: u64,
    pub handshake_failures: u64,
    pub avg_session_duration_secs: f64,
}

/// Comprehensive session manager
pub struct SessionManager {
    // Session storage
    sessions: Arc<DashMap<ConnectionId, Session>>,
    
    // Lookup indices
    peer_to_sessions: Arc<DashMap<NodeId, Vec<ConnectionId>>>,
    
    // Statistics
    statistics: Arc<RwLock<SessionStatistics>>,
    
    // Configuration
    config: SessionManagerConfig,
    
    // Background tasks
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: SessionManagerConfig) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            peer_to_sessions: Arc::new(DashMap::new()),
            statistics: Arc::new(RwLock::new(SessionStatistics::default())),
            config,
            cleanup_task: None,
        }
    }
    
    /// Start the session manager
    pub async fn start(&mut self) -> anyhow::Result<()> {
        // Start cleanup task
        let cleanup_task = {
            let manager = self.clone();
            tokio::spawn(async move {
                manager.cleanup_loop().await;
            })
        };
        self.cleanup_task = Some(cleanup_task);
        
        info!("Session manager started with {} max sessions", self.config.max_sessions);
        Ok(())
    }
    
    /// Create a new session
    #[instrument(skip(self))]
    pub async fn create_session(&self, peer_node_id: Option<NodeId>) -> anyhow::Result<ConnectionId> {
        // Check session limit
        if self.sessions.len() >= self.config.max_sessions as usize {
            return Err(anyhow::anyhow!("Session limit reached"));
        }
        
        // Generate unique CID
        let cid = self.generate_cid();
        
        // Create session
        let session = Session {
            cid,
            peer_node_id,
            state: SessionState::Initializing,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            stream_count: 0,
            bytes_sent: 0,
            bytes_received: 0,
            crypter: None,
            metadata: HashMap::new(),
        };
        
        // Store session
        self.sessions.insert(cid, session.clone());
        
        // Update peer index if peer is known
        if let Some(peer_id) = peer_node_id {
            self.peer_to_sessions
                .entry(peer_id)
                .or_insert_with(Vec::new)
                .push(cid);
        }
        
        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.total_sessions_created += 1;
            stats.active_sessions = self.sessions.len() as u32;
        }
        
        info!("Created session {} for peer {:?}", hex::encode(cid), peer_node_id);
        Ok(cid)
    }
    
    /// Get session by CID
    pub fn get_session(&self, cid: &ConnectionId) -> Option<Session> {
        self.sessions.get(cid).map(|entry| entry.clone())
    }
    
    /// Update session activity
    #[instrument(skip(self))]
    pub async fn update_activity(&self, cid: &ConnectionId) -> anyhow::Result<()> {
        if let Some(mut session) = self.sessions.get_mut(cid) {
            session.last_activity = SystemTime::now();
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Update session state
    #[instrument(skip(self), fields(cid = %hex::encode(cid)))]
    pub async fn update_state(&self, cid: &ConnectionId, new_state: SessionState) -> anyhow::Result<()> {
        if let Some(mut session) = self.sessions.get_mut(cid) {
            let old_state = session.state.clone();
            session.state = new_state.clone();
            session.last_activity = SystemTime::now();
            
            debug!("Session {} state changed: {:?} -> {:?}", 
                   hex::encode(cid), old_state, new_state);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Set session crypter
    pub async fn set_crypter(&self, cid: &ConnectionId, crypter: Arc<FrameCrypter>) -> anyhow::Result<()> {
        if let Some(mut session) = self.sessions.get_mut(cid) {
            session.crypter = Some(crypter);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Increment stream count for session
    pub async fn increment_stream_count(&self, cid: &ConnectionId) -> anyhow::Result<()> {
        if let Some(mut session) = self.sessions.get_mut(cid) {
            if session.stream_count >= self.config.max_streams_per_session {
                return Err(anyhow::anyhow!("Stream limit reached for session"));
            }
            session.stream_count += 1;
            session.last_activity = SystemTime::now();
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Decrement stream count for session
    pub async fn decrement_stream_count(&self, cid: &ConnectionId) -> anyhow::Result<()> {
        if let Some(mut session) = self.sessions.get_mut(cid) {
            if session.stream_count > 0 {
                session.stream_count -= 1;
            }
            session.last_activity = SystemTime::now();
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Update session traffic statistics
    pub async fn update_traffic_stats(&self, cid: &ConnectionId, bytes_sent: u64, bytes_received: u64) -> anyhow::Result<()> {
        if let Some(mut session) = self.sessions.get_mut(cid) {
            session.bytes_sent += bytes_sent;
            session.bytes_received += bytes_received;
            session.last_activity = SystemTime::now();
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Close session
    #[instrument(skip(self), fields(cid = %hex::encode(cid)))]
    pub async fn close_session(&self, cid: &ConnectionId) -> anyhow::Result<()> {
        if let Some(session) = self.sessions.get(cid) {
            let peer_node_id = session.peer_node_id;
            
            // Update state to closing
            if let Some(mut session) = self.sessions.get_mut(cid) {
                session.state = SessionState::Closing;
                session.last_activity = SystemTime::now();
            }
            
            // Remove from peer index
            if let Some(peer_id) = peer_node_id {
                if let Some(mut sessions) = self.peer_to_sessions.get_mut(&peer_id) {
                    sessions.retain(|&id| id != *cid);
                    if sessions.is_empty() {
                        drop(sessions);
                        self.peer_to_sessions.remove(&peer_id);
                    }
                }
            }
            
            // Remove session
            self.sessions.remove(cid);
            
            // Update statistics
            {
                let mut stats = self.statistics.write().await;
                stats.active_sessions = self.sessions.len() as u32;
            }
            
            info!("Closed session {}", hex::encode(cid));
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }
    
    /// Get sessions for a peer
    pub fn get_peer_sessions(&self, peer_id: &NodeId) -> Vec<ConnectionId> {
        self.peer_to_sessions
            .get(peer_id)
            .map(|sessions| sessions.clone())
            .unwrap_or_default()
    }
    
    /// Get all active sessions
    pub fn get_active_sessions(&self) -> Vec<Session> {
        self.sessions
            .iter()
            .filter(|entry| entry.state == SessionState::Active)
            .map(|entry| entry.clone())
            .collect()
    }
    
    /// Get session statistics
    pub async fn get_statistics(&self) -> SessionStatistics {
        let mut stats = self.statistics.read().await.clone();
        stats.active_sessions = self.sessions.len() as u32;
        
        // Calculate average session duration
        let now = SystemTime::now();
        let mut total_duration = Duration::new(0, 0);
        let mut session_count = 0;
        
        for session in self.sessions.iter() {
            if let Ok(duration) = now.duration_since(session.created_at) {
                total_duration += duration;
                session_count += 1;
            }
        }
        
        if session_count > 0 {
            stats.avg_session_duration_secs = total_duration.as_secs_f64() / session_count as f64;
        }
        
        stats
    }
    
    /// Generate a unique 96-bit Connection ID
    fn generate_cid(&self) -> ConnectionId {
        loop {
            let mut cid = [0u8; 12];
            rand::thread_rng().fill_bytes(&mut cid);
            
            // Ensure uniqueness
            if !self.sessions.contains_key(&cid) {
                return cid;
            }
        }
    }
    
    /// Background cleanup loop
    async fn cleanup_loop(&self) {
        let mut interval = interval(Duration::from_secs(self.config.cleanup_interval_secs));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.cleanup_expired_sessions().await {
                error!("Session cleanup failed: {}", e);
            }
        }
    }
    
    /// Clean up expired sessions
    async fn cleanup_expired_sessions(&self) -> anyhow::Result<()> {
        let now = SystemTime::now();
        let timeout = Duration::from_secs(self.config.session_timeout_secs);
        let mut expired_sessions = Vec::new();
        
        // Find expired sessions
        for entry in self.sessions.iter() {
            let session = entry.value();
            if let Ok(duration) = now.duration_since(session.last_activity) {
                if duration > timeout {
                    expired_sessions.push(session.cid);
                }
            }
        }
        
        // Remove expired sessions
        for cid in expired_sessions {
            if let Err(e) = self.close_session(&cid).await {
                warn!("Failed to close expired session {}: {}", hex::encode(cid), e);
            } else {
                debug!("Cleaned up expired session {}", hex::encode(cid));
            }
        }
        
        // Update statistics
        {
            let mut stats = self.statistics.write().await;
            stats.expired_sessions += expired_sessions.len() as u64;
        }
        
        if !expired_sessions.is_empty() {
            info!("Cleaned up {} expired sessions", expired_sessions.len());
        }
        
        Ok(())
    }
    
    /// Validate CID format
    pub fn is_valid_cid(cid: &[u8]) -> bool {
        cid.len() == 12 && !cid.iter().all(|&b| b == 0)
    }
    
    /// Convert CID to hex string
    pub fn cid_to_string(cid: &ConnectionId) -> String {
        hex::encode(cid)
    }
    
    /// Parse CID from hex string
    pub fn string_to_cid(s: &str) -> anyhow::Result<ConnectionId> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 12 {
            return Err(anyhow::anyhow!("Invalid CID length"));
        }
        
        let mut cid = [0u8; 12];
        cid.copy_from_slice(&bytes);
        Ok(cid)
    }
}

impl Clone for SessionManager {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            peer_to_sessions: Arc::clone(&self.peer_to_sessions),
            statistics: Arc::clone(&self.statistics),
            config: self.config.clone(),
            cleanup_task: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_session_creation() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);
        
        let peer_id = [1u8; 32];
        let cid = manager.create_session(Some(peer_id)).await.unwrap();
        
        assert!(manager.get_session(&cid).is_some());
        assert_eq!(manager.get_peer_sessions(&peer_id), vec![cid]);
    }
    
    #[tokio::test]
    async fn test_session_lifecycle() {
        let config = SessionManagerConfig::default();
        let manager = SessionManager::new(config);
        
        let cid = manager.create_session(None).await.unwrap();
        
        // Update state
        manager.update_state(&cid, SessionState::Active).await.unwrap();
        let session = manager.get_session(&cid).unwrap();
        assert_eq!(session.state, SessionState::Active);
        
        // Close session
        manager.close_session(&cid).await.unwrap();
        assert!(manager.get_session(&cid).is_none());
    }
    
    #[test]
    fn test_cid_validation() {
        let valid_cid = [1u8; 12];
        let invalid_cid = [0u8; 12];
        
        assert!(SessionManager::is_valid_cid(&valid_cid));
        assert!(!SessionManager::is_valid_cid(&invalid_cid));
    }
    
    #[test]
    fn test_cid_string_conversion() {
        let cid = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67];
        let cid_str = SessionManager::cid_to_string(&cid);
        let parsed_cid = SessionManager::string_to_cid(&cid_str).unwrap();
        
        assert_eq!(cid, parsed_cid);
    }
} 