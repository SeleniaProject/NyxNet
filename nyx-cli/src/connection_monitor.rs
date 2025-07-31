#![forbid(unsafe_code)]

//! Connection quality monitoring and automatic reconnection for the Nyx CLI.
//!
//! This module provides comprehensive connection monitoring with quality metrics,
//! automatic reconnection with exponential backoff, and connection state notifications.

use crate::proto::nyx_control_client::NyxControlClient;
use crate::proto::{Empty, StreamId};
use crate::create_authenticated_request;
use crate::Cli;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex, mpsc};
use tokio::time::{sleep, interval};
use tonic::transport::Channel;
use tracing::{debug, info, warn, error};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Connection quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionQuality {
    /// Current connection status
    pub status: ConnectionStatus,
    /// Average latency over the monitoring window
    pub avg_latency_ms: f64,
    /// Packet loss rate (0.0 to 1.0)
    pub packet_loss_rate: f64,
    /// Connection stability score (0.0 to 100.0)
    pub stability_score: f64,
    /// Number of successful health checks
    pub successful_checks: u64,
    /// Number of failed health checks
    pub failed_checks: u64,
    /// Last successful check time
    pub last_success: Option<DateTime<Utc>>,
    /// Last failure time
    pub last_failure: Option<DateTime<Utc>>,
    /// Connection uptime
    pub uptime: Duration,
}

/// Connection status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Connection is healthy and stable
    Healthy,
    /// Connection is degraded but functional
    Degraded,
    /// Connection is unstable
    Unstable,
    /// Connection is disconnected
    Disconnected,
    /// Currently reconnecting
    Reconnecting,
    /// Connection failed permanently
    Failed,
}

/// Connection event for notifications
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// Connection established
    Connected,
    /// Connection lost
    Disconnected { reason: String },
    /// Connection quality changed
    QualityChanged { old_quality: ConnectionQuality, new_quality: ConnectionQuality },
    /// Reconnection started
    ReconnectionStarted { attempt: u32 },
    /// Reconnection succeeded
    ReconnectionSucceeded { attempt: u32, duration: Duration },
    /// Reconnection failed
    ReconnectionFailed { attempt: u32, error: String },
    /// Maximum reconnection attempts reached
    ReconnectionExhausted { total_attempts: u32 },
}

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectionConfig {
    /// Enable automatic reconnection
    pub enabled: bool,
    /// Maximum number of reconnection attempts
    pub max_attempts: u32,
    /// Initial delay between reconnection attempts
    pub initial_delay: Duration,
    /// Maximum delay between reconnection attempts
    pub max_delay: Duration,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Add jitter to reconnection delays
    pub jitter: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Connection timeout for health checks
    pub health_check_timeout: Duration,
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 5,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
            health_check_interval: Duration::from_secs(10),
            health_check_timeout: Duration::from_secs(5),
        }
    }
}

/// Connection monitor that tracks quality and handles reconnection
pub struct ConnectionMonitor {
    client: Arc<Mutex<NyxControlClient<Channel>>>,
    cli: Cli,
    config: ReconnectionConfig,
    quality: Arc<RwLock<ConnectionQuality>>,
    shutdown: Arc<AtomicBool>,
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    reconnection_state: Arc<RwLock<ReconnectionState>>,
}

/// Internal reconnection state
#[derive(Debug, Clone)]
struct ReconnectionState {
    /// Current attempt number
    pub attempt: u32,
    /// Time when reconnection started
    pub started_at: Option<Instant>,
    /// Next reconnection delay
    pub next_delay: Duration,
    /// Whether reconnection is in progress
    pub in_progress: bool,
}

impl Default for ReconnectionState {
    fn default() -> Self {
        Self {
            attempt: 0,
            started_at: None,
            next_delay: Duration::from_millis(500),
            in_progress: false,
        }
    }
}

impl ConnectionMonitor {
    /// Create a new connection monitor
    pub fn new(
        client: Arc<Mutex<NyxControlClient<Channel>>>,
        cli: Cli,
        config: ReconnectionConfig,
    ) -> (Self, mpsc::UnboundedReceiver<ConnectionEvent>) {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        
        let monitor = Self {
            client,
            cli,
            config,
            quality: Arc::new(RwLock::new(ConnectionQuality {
                status: ConnectionStatus::Disconnected,
                avg_latency_ms: 0.0,
                packet_loss_rate: 0.0,
                stability_score: 0.0,
                successful_checks: 0,
                failed_checks: 0,
                last_success: None,
                last_failure: None,
                uptime: Duration::ZERO,
            })),
            shutdown: Arc::new(AtomicBool::new(false)),
            event_sender,
            reconnection_state: Arc::new(RwLock::new(ReconnectionState::default())),
        };
        
        (monitor, event_receiver)
    }
    
    /// Start monitoring the connection
    pub async fn start(&self) {
        info!("Starting connection monitor");
        
        let client = self.client.clone();
        let cli = self.cli.clone();
        let config = self.config.clone();
        let quality = self.quality.clone();
        let shutdown = self.shutdown.clone();
        let event_sender = self.event_sender.clone();
        let reconnection_state = self.reconnection_state.clone();
        
        tokio::spawn(async move {
            let mut health_check_interval = interval(config.health_check_interval);
            let connection_start = Instant::now();
            
            while !shutdown.load(Ordering::Relaxed) {
                health_check_interval.tick().await;
                
                // Perform health check
                let health_result = Self::perform_health_check(&client, &cli, config.health_check_timeout).await;
                
                // Update quality metrics
                let mut quality_guard = quality.write().await;
                let old_quality = quality_guard.clone();
                
                match health_result {
                    Ok(latency) => {
                        // Successful health check
                        quality_guard.successful_checks += 1;
                        quality_guard.last_success = Some(Utc::now());
                        quality_guard.uptime = connection_start.elapsed();
                        
                        // Update latency with exponential moving average
                        if quality_guard.avg_latency_ms == 0.0 {
                            quality_guard.avg_latency_ms = latency;
                        } else {
                            quality_guard.avg_latency_ms = quality_guard.avg_latency_ms * 0.8 + latency * 0.2;
                        }
                        
                        // Calculate stability score
                        let total_checks = quality_guard.successful_checks + quality_guard.failed_checks;
                        if total_checks > 0 {
                            quality_guard.stability_score = (quality_guard.successful_checks as f64 / total_checks as f64) * 100.0;
                        }
                        
                        // Update status based on metrics
                        quality_guard.status = if quality_guard.stability_score >= 95.0 && quality_guard.avg_latency_ms < 100.0 {
                            ConnectionStatus::Healthy
                        } else if quality_guard.stability_score >= 80.0 && quality_guard.avg_latency_ms < 500.0 {
                            ConnectionStatus::Degraded
                        } else {
                            ConnectionStatus::Unstable
                        };
                        
                        // Reset reconnection state on successful check
                        {
                            let mut reconnect_state = reconnection_state.write().await;
                            if reconnect_state.in_progress {
                                reconnect_state.in_progress = false;
                                reconnect_state.attempt = 0;
                                reconnect_state.next_delay = config.initial_delay;
                                
                                let _ = event_sender.send(ConnectionEvent::ReconnectionSucceeded {
                                    attempt: reconnect_state.attempt,
                                    duration: reconnect_state.started_at.map(|start| start.elapsed()).unwrap_or(Duration::ZERO),
                                });
                            }
                        }
                        
                        debug!("Health check successful: {:.2}ms latency", latency);
                    }
                    Err(error) => {
                        // Failed health check
                        quality_guard.failed_checks += 1;
                        quality_guard.last_failure = Some(Utc::now());
                        quality_guard.status = ConnectionStatus::Disconnected;
                        
                        // Calculate packet loss rate (simplified)
                        let total_checks = quality_guard.successful_checks + quality_guard.failed_checks;
                        if total_checks > 0 {
                            quality_guard.packet_loss_rate = quality_guard.failed_checks as f64 / total_checks as f64;
                            quality_guard.stability_score = (quality_guard.successful_checks as f64 / total_checks as f64) * 100.0;
                        }
                        
                        warn!("Health check failed: {}", error);
                        
                        // Trigger reconnection if enabled
                        if config.enabled {
                            let should_reconnect = {
                                let mut reconnect_state = reconnection_state.write().await;
                                if !reconnect_state.in_progress && reconnect_state.attempt < config.max_attempts {
                                    reconnect_state.in_progress = true;
                                    reconnect_state.attempt += 1;
                                    reconnect_state.started_at = Some(Instant::now());
                                    true
                                } else {
                                    false
                                }
                            };
                            
                            if should_reconnect {
                                let reconnect_client = client.clone();
                                let reconnect_cli = cli.clone();
                                let reconnect_config = config.clone();
                                let reconnect_quality = quality.clone();
                                let reconnect_event_sender = event_sender.clone();
                                let reconnect_state_clone = reconnection_state.clone();
                                
                                tokio::spawn(async move {
                                    Self::handle_reconnection(
                                        reconnect_client,
                                        reconnect_cli,
                                        reconnect_config,
                                        reconnect_quality,
                                        reconnect_event_sender,
                                        reconnect_state_clone,
                                    ).await;
                                });
                            }
                        }
                    }
                }
                
                // Send quality change event if status changed
                if old_quality.status != quality_guard.status {
                    let _ = event_sender.send(ConnectionEvent::QualityChanged {
                        old_quality,
                        new_quality: quality_guard.clone(),
                    });
                }
            }
        });
    }
    
    /// Stop monitoring
    pub fn stop(&self) {
        info!("Stopping connection monitor");
        self.shutdown.store(true, Ordering::Relaxed);
    }
    
    /// Get current connection quality
    pub async fn get_quality(&self) -> ConnectionQuality {
        self.quality.read().await.clone()
    }
    
    /// Perform a health check against the daemon
    async fn perform_health_check(
        client: &Arc<Mutex<NyxControlClient<Channel>>>,
        cli: &Cli,
        timeout: Duration,
    ) -> Result<f64, String> {
        let start = Instant::now();
        
        let result = tokio::time::timeout(timeout, async {
            let mut client_guard = client.lock().await;
            let request = create_authenticated_request(cli, Empty {});
            client_guard.get_info(request).await
        }).await;
        
        match result {
            Ok(Ok(_)) => {
                let latency = start.elapsed().as_millis() as f64;
                Ok(latency)
            }
            Ok(Err(e)) => Err(format!("gRPC error: {}", e)),
            Err(_) => Err("Health check timeout".to_string()),
        }
    }
    
    /// Handle reconnection with exponential backoff
    async fn handle_reconnection(
        client: Arc<Mutex<NyxControlClient<Channel>>>,
        cli: Cli,
        config: ReconnectionConfig,
        quality: Arc<RwLock<ConnectionQuality>>,
        event_sender: mpsc::UnboundedSender<ConnectionEvent>,
        reconnection_state: Arc<RwLock<ReconnectionState>>,
    ) {
        let attempt = {
            let state = reconnection_state.read().await;
            state.attempt
        };
        
        info!("Starting reconnection attempt {}/{}", attempt, config.max_attempts);
        let _ = event_sender.send(ConnectionEvent::ReconnectionStarted { attempt });
        
        // Calculate delay with exponential backoff and jitter
        let delay = {
            let mut state = reconnection_state.write().await;
            let base_delay = config.initial_delay.as_millis() as f64 * config.backoff_multiplier.powi((attempt - 1) as i32);
            let capped_delay = base_delay.min(config.max_delay.as_millis() as f64);
            
            let final_delay = if config.jitter {
                let jitter_factor = 0.8 + (fastrand::f64() * 0.4); // 0.8 to 1.2
                Duration::from_millis((capped_delay * jitter_factor) as u64)
            } else {
                Duration::from_millis(capped_delay as u64)
            };
            
            state.next_delay = final_delay;
            final_delay
        };
        
        // Wait before attempting reconnection
        sleep(delay).await;
        
        // Update status to reconnecting
        {
            let mut quality_guard = quality.write().await;
            quality_guard.status = ConnectionStatus::Reconnecting;
        }
        
        // Attempt to reconnect by performing a health check
        match Self::perform_health_check(&client, &cli, config.health_check_timeout).await {
            Ok(latency) => {
                info!("Reconnection attempt {} succeeded with {:.2}ms latency", attempt, latency);
                
                // Update quality metrics
                {
                    let mut quality_guard = quality.write().await;
                    quality_guard.status = ConnectionStatus::Healthy;
                    quality_guard.avg_latency_ms = latency;
                    quality_guard.last_success = Some(Utc::now());
                }
                
                // Reset reconnection state
                {
                    let mut state = reconnection_state.write().await;
                    let duration = state.started_at.map(|start| start.elapsed()).unwrap_or(Duration::ZERO);
                    state.in_progress = false;
                    state.attempt = 0;
                    state.next_delay = config.initial_delay;
                    
                    let _ = event_sender.send(ConnectionEvent::ReconnectionSucceeded { attempt, duration });
                }
            }
            Err(error) => {
                warn!("Reconnection attempt {} failed: {}", attempt, error);
                let _ = event_sender.send(ConnectionEvent::ReconnectionFailed { attempt, error });
                
                // Check if we should continue trying
                if attempt >= config.max_attempts {
                    error!("Maximum reconnection attempts ({}) reached", config.max_attempts);
                    
                    {
                        let mut quality_guard = quality.write().await;
                        quality_guard.status = ConnectionStatus::Failed;
                    }
                    
                    {
                        let mut state = reconnection_state.write().await;
                        state.in_progress = false;
                    }
                    
                    let _ = event_sender.send(ConnectionEvent::ReconnectionExhausted { 
                        total_attempts: config.max_attempts 
                    });
                } else {
                    // Schedule next attempt
                    let next_client = client.clone();
                    let next_cli = cli.clone();
                    let next_config = config.clone();
                    let next_quality = quality.clone();
                    let next_event_sender = event_sender.clone();
                    let next_reconnection_state = reconnection_state.clone();
                    
                    tokio::spawn(async move {
                        Self::handle_reconnection(
                            next_client,
                            next_cli,
                            next_config,
                            next_quality,
                            next_event_sender,
                            next_reconnection_state,
                        ).await;
                    });
                }
            }
        }
    }
}

/// Connection quality display utilities
impl ConnectionQuality {
    /// Get a human-readable status description
    pub fn status_description(&self) -> &'static str {
        match self.status {
            ConnectionStatus::Healthy => "Healthy - Connection is stable and performing well",
            ConnectionStatus::Degraded => "Degraded - Connection is functional but experiencing issues",
            ConnectionStatus::Unstable => "Unstable - Connection is unreliable",
            ConnectionStatus::Disconnected => "Disconnected - No connection to daemon",
            ConnectionStatus::Reconnecting => "Reconnecting - Attempting to restore connection",
            ConnectionStatus::Failed => "Failed - Connection cannot be established",
        }
    }
    
    /// Get a color for status display
    pub fn status_color(&self) -> console::Color {
        match self.status {
            ConnectionStatus::Healthy => console::Color::Green,
            ConnectionStatus::Degraded => console::Color::Yellow,
            ConnectionStatus::Unstable => console::Color::Magenta,
            ConnectionStatus::Disconnected => console::Color::Red,
            ConnectionStatus::Reconnecting => console::Color::Cyan,
            ConnectionStatus::Failed => console::Color::Red,
        }
    }
    
    /// Get quality grade (A-F)
    pub fn quality_grade(&self) -> char {
        if self.stability_score >= 95.0 && self.avg_latency_ms < 50.0 {
            'A'
        } else if self.stability_score >= 90.0 && self.avg_latency_ms < 100.0 {
            'B'
        } else if self.stability_score >= 80.0 && self.avg_latency_ms < 200.0 {
            'C'
        } else if self.stability_score >= 70.0 && self.avg_latency_ms < 500.0 {
            'D'
        } else {
            'F'
        }
    }
}