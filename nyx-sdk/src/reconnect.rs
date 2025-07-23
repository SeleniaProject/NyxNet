#![forbid(unsafe_code)]

//! Reconnection management for the Nyx SDK.
//!
//! This module provides automatic reconnection capabilities with exponential backoff,
//! circuit breaker pattern, and comprehensive retry logic for resilient connections.

#[cfg(feature = "reconnect")]
use crate::error::{NyxError, NyxResult};
#[cfg(feature = "reconnect")]
use crate::proto::nyx_control_client::NyxControlClient;
#[cfg(feature = "reconnect")]
use crate::proto::OpenRequest;
#[cfg(feature = "reconnect")]
use crate::stream::StreamOptions;

#[cfg(feature = "reconnect")]
use std::sync::Arc;
#[cfg(feature = "reconnect")]
use std::time::{Duration, Instant};
#[cfg(feature = "reconnect")]
use tokio::sync::{Mutex, RwLock};
#[cfg(feature = "reconnect")]
use tonic::transport::Channel;
#[cfg(feature = "reconnect")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "reconnect")]
use backoff::{ExponentialBackoff, backoff::Backoff};
#[cfg(feature = "reconnect")]
use chrono::{DateTime, Utc};
#[cfg(feature = "reconnect")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "reconnect")]
/// Reconnection manager that handles automatic stream reconnection
pub struct ReconnectionManager {
    target: String,
    options: StreamOptions,
    state: Arc<RwLock<ReconnectionState>>,
    backoff: Arc<Mutex<ExponentialBackoff>>,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    stats: Arc<RwLock<ReconnectionStats>>,
}

#[cfg(feature = "reconnect")]
/// Current state of the reconnection manager
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconnectionState {
    /// Not currently reconnecting
    Idle,
    /// Reconnection in progress
    Reconnecting { attempt: u32, started_at: Instant },
    /// Reconnection temporarily suspended due to circuit breaker
    Suspended { until: Instant },
    /// Reconnection permanently failed
    Failed { reason: String },
}

#[cfg(feature = "reconnect")]
/// Circuit breaker for managing reconnection attempts
#[derive(Debug, Clone)]
struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    last_failure: Option<Instant>,
    success_count: u32,
    config: CircuitBreakerConfig,
}

#[cfg(feature = "reconnect")]
/// Circuit breaker state
#[derive(Debug, Clone, PartialEq, Eq)]
enum CircuitBreakerState {
    /// Circuit is closed, allowing requests
    Closed,
    /// Circuit is open, blocking requests
    Open { opened_at: Instant },
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

#[cfg(feature = "reconnect")]
/// Configuration for circuit breaker
#[derive(Debug, Clone)]
struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    failure_threshold: u32,
    /// Duration to keep circuit open
    timeout: Duration,
    /// Number of successful requests needed to close circuit from half-open
    success_threshold: u32,
}

#[cfg(feature = "reconnect")]
/// Statistics for reconnection attempts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconnectionStats {
    /// Total number of reconnection attempts
    pub total_attempts: u32,
    /// Number of successful reconnections
    pub successful_reconnections: u32,
    /// Number of failed reconnections
    pub failed_reconnections: u32,
    /// Time of last reconnection attempt
    pub last_attempt: Option<DateTime<Utc>>,
    /// Time of last successful reconnection
    pub last_success: Option<DateTime<Utc>>,
    /// Current consecutive failure count
    pub consecutive_failures: u32,
    /// Total time spent reconnecting
    pub total_reconnection_time: Duration,
    /// Average time per successful reconnection
    pub avg_reconnection_time: Duration,
}

#[cfg(feature = "reconnect")]
impl ReconnectionManager {
    /// Create a new reconnection manager
    pub fn new(target: String, options: StreamOptions) -> Self {
        let backoff = ExponentialBackoff {
            initial_interval: options.reconnect_delay,
            max_interval: Duration::from_secs(300), // 5 minutes max
            max_elapsed_time: Some(Duration::from_secs(3600)), // 1 hour total
            multiplier: 2.0,
            randomization_factor: 0.1,
            ..Default::default()
        };

        let circuit_breaker = CircuitBreaker {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure: None,
            success_count: 0,
            config: CircuitBreakerConfig {
                failure_threshold: 5,
                timeout: Duration::from_secs(60),
                success_threshold: 2,
            },
        };

        Self {
            target,
            options,
            state: Arc::new(RwLock::new(ReconnectionState::Idle)),
            backoff: Arc::new(Mutex::new(backoff)),
            circuit_breaker: Arc::new(RwLock::new(circuit_breaker)),
            stats: Arc::new(RwLock::new(ReconnectionStats::default())),
        }
    }

    /// Attempt to reconnect the stream
    pub async fn reconnect(
        &self,
        client: &Arc<Mutex<NyxControlClient<Channel>>>,
    ) -> NyxResult<u32> {
        // Check circuit breaker
        {
            let mut breaker = self.circuit_breaker.write().await;
            if !breaker.can_attempt() {
                return Err(NyxError::ReconnectionFailed { 
                    attempts: self.stats.read().await.total_attempts 
                });
            }
        }

        let start_time = Instant::now();
        let mut attempt = 1;

        // Update state to reconnecting
        {
            let mut state = self.state.write().await;
            *state = ReconnectionState::Reconnecting {
                attempt,
                started_at: start_time,
            };
        }

        loop {
            // Check if we've exceeded max attempts
            if attempt > self.options.max_reconnect_attempts {
                self.record_failure().await;
                let mut state = self.state.write().await;
                *state = ReconnectionState::Failed {
                    reason: "Maximum reconnection attempts exceeded".to_string(),
                };
                return Err(NyxError::ReconnectionFailed { attempts: attempt });
            }

            debug!("Reconnection attempt {} for target {}", attempt, self.target);

            // Attempt to reconnect
            match self.attempt_reconnection(client).await {
                Ok(new_stream_id) => {
                    let reconnection_time = start_time.elapsed();
                    self.record_success(reconnection_time).await;
                    
                    // Reset state to idle
                    {
                        let mut state = self.state.write().await;
                        *state = ReconnectionState::Idle;
                    }
                    
                    info!(
                        "Successfully reconnected to {} with stream ID {} after {} attempts in {:?}",
                        self.target, new_stream_id, attempt, reconnection_time
                    );
                    
                    return Ok(new_stream_id);
                }
                Err(e) => {
                    warn!("Reconnection attempt {} failed: {}", attempt, e);
                    
                    // Check if error is retryable
                    if !e.is_retryable() {
                        self.record_failure().await;
                        let mut state = self.state.write().await;
                        *state = ReconnectionState::Failed {
                            reason: e.to_string(),
                        };
                        return Err(e);
                    }
                    
                    // Wait before next attempt
                    if attempt < self.options.max_reconnect_attempts {
                        let delay = self.get_next_delay().await;
                        debug!("Waiting {:?} before next reconnection attempt", delay);
                        tokio::time::sleep(delay).await;
                    }
                    
                    attempt += 1;
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        *state = ReconnectionState::Reconnecting {
                            attempt,
                            started_at: start_time,
                        };
                    }
                }
            }
        }
    }

    /// Attempt a single reconnection
    async fn attempt_reconnection(
        &self,
        client: &Arc<Mutex<NyxControlClient<Channel>>>,
    ) -> NyxResult<u32> {
        let mut client_guard = client.lock().await;
        
        let request = OpenRequest {
            stream_name: self.target.clone(),
        };
        
        let timeout = self.options.operation_timeout;
        let response = tokio::time::timeout(timeout, client_guard.open_stream(request))
            .await
            .map_err(|_| NyxError::timeout(timeout))?
            .map_err(NyxError::from)?;
        
        let stream_response = response.into_inner();
        Ok(stream_response.stream_id)
    }

    /// Get the next delay duration using exponential backoff
    async fn get_next_delay(&self) -> Duration {
        let mut backoff = self.backoff.lock().await;
        backoff.next_backoff().unwrap_or(Duration::from_secs(60))
    }

    /// Record a successful reconnection
    async fn record_success(&self, reconnection_time: Duration) {
        // Update circuit breaker
        {
            let mut breaker = self.circuit_breaker.write().await;
            breaker.record_success();
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.successful_reconnections += 1;
            stats.consecutive_failures = 0;
            stats.last_success = Some(Utc::now());
            stats.total_reconnection_time += reconnection_time;
            
            if stats.successful_reconnections > 0 {
                stats.avg_reconnection_time = stats.total_reconnection_time / stats.successful_reconnections;
            }
        }
        
        // Reset backoff
        {
            let mut backoff = self.backoff.lock().await;
            backoff.reset();
        }
    }

    /// Record a failed reconnection
    async fn record_failure(&self) {
        // Update circuit breaker
        {
            let mut breaker = self.circuit_breaker.write().await;
            breaker.record_failure();
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.failed_reconnections += 1;
            stats.consecutive_failures += 1;
            stats.last_attempt = Some(Utc::now());
        }
    }

    /// Get current reconnection statistics
    pub async fn stats(&self) -> ReconnectionStats {
        self.stats.read().await.clone()
    }

    /// Get current reconnection state
    pub async fn state(&self) -> String {
        let state = self.state.read().await;
        match *state {
            ReconnectionState::Idle => "idle".to_string(),
            ReconnectionState::Reconnecting { attempt, .. } => {
                format!("reconnecting (attempt {})", attempt)
            }
            ReconnectionState::Suspended { until } => {
                format!("suspended until {:?}", until)
            }
            ReconnectionState::Failed { ref reason } => {
                format!("failed: {}", reason)
            }
        }
    }

    /// Check if reconnection is currently in progress
    pub async fn is_reconnecting(&self) -> bool {
        matches!(*self.state.read().await, ReconnectionState::Reconnecting { .. })
    }

    /// Reset the reconnection manager
    pub async fn reset(&self) {
        {
            let mut state = self.state.write().await;
            *state = ReconnectionState::Idle;
        }
        
        {
            let mut backoff = self.backoff.lock().await;
            backoff.reset();
        }
        
        {
            let mut breaker = self.circuit_breaker.write().await;
            breaker.reset();
        }
    }
}

#[cfg(feature = "reconnect")]
impl CircuitBreaker {
    /// Check if an attempt can be made
    fn can_attempt(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open { opened_at } => {
                if opened_at.elapsed() >= self.config.timeout {
                    // Transition to half-open
                    self.state = CircuitBreakerState::HalfOpen;
                    self.success_count = 0;
                    true
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    /// Record a successful operation
    fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    // Transition back to closed
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                }
            }
            CircuitBreakerState::Open { .. } => {
                // Should not happen
            }
        }
    }

    /// Record a failed operation
    fn record_failure(&mut self) {
        self.last_failure = Some(Instant::now());
        
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    // Transition to open
                    self.state = CircuitBreakerState::Open {
                        opened_at: Instant::now(),
                    };
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Transition back to open
                self.state = CircuitBreakerState::Open {
                    opened_at: Instant::now(),
                };
                self.success_count = 0;
            }
            CircuitBreakerState::Open { .. } => {
                // Already open, nothing to do
            }
        }
    }

    /// Reset the circuit breaker
    fn reset(&mut self) {
        self.state = CircuitBreakerState::Closed;
        self.failure_count = 0;
        self.last_failure = None;
        self.success_count = 0;
    }
}

// Stub implementations for when reconnect feature is not enabled
#[cfg(not(feature = "reconnect"))]
pub struct ReconnectionManager;

#[cfg(not(feature = "reconnect"))]
impl ReconnectionManager {
    pub fn new(_target: String, _options: crate::stream::StreamOptions) -> Self {
        Self
    }
} 