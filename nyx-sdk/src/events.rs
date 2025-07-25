#![forbid(unsafe_code)]

//! Event system for the Nyx SDK.
//!
//! This module provides an event-driven interface for monitoring SDK operations,
//! connection status changes, and stream lifecycle events.

use crate::daemon::{ConnectionStatus, HealthStatus};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

/// SDK events that can be monitored
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NyxEvent {
    /// Daemon connection established
    DaemonConnected {
        node_id: String,
        version: String,
        connected_at: DateTime<Utc>,
    },
    
    /// Daemon connection lost
    DaemonDisconnected {
        reason: String,
        disconnected_at: DateTime<Utc>,
    },
    
    /// Daemon connection status changed
    DaemonStatusChanged {
        old_status: ConnectionStatus,
        new_status: ConnectionStatus,
        changed_at: DateTime<Utc>,
    },
    
    /// Daemon health status changed
    DaemonHealthChanged {
        old_health: HealthStatus,
        new_health: HealthStatus,
        changed_at: DateTime<Utc>,
    },
    
    /// Stream opened successfully
    StreamOpened {
        stream_id: u32,
        target: String,
    },
    
    /// Stream closed
    StreamClosed {
        stream_id: u32,
    },
    
    /// Stream error occurred
    StreamError {
        stream_id: u32,
        error: String,
        error_at: DateTime<Utc>,
    },
    
    /// Stream reconnection started
    #[cfg(feature = "reconnect")]
    StreamReconnecting {
        stream_id: u32,
        attempt: u32,
        started_at: DateTime<Utc>,
    },
    
    /// Stream reconnection completed
    #[cfg(feature = "reconnect")]
    StreamReconnected {
        old_stream_id: u32,
        new_stream_id: u32,
        reconnected_at: DateTime<Utc>,
    },
    
    /// Configuration changed
    ConfigChanged {
        changed_at: DateTime<Utc>,
    },
    
    /// Network path added
    NetworkPathAdded {
        path_id: u8,
        endpoint: String,
        added_at: DateTime<Utc>,
    },
    
    /// Network path removed
    NetworkPathRemoved {
        path_id: u8,
        endpoint: String,
        removed_at: DateTime<Utc>,
    },
    
    /// Metrics updated
    MetricsUpdated {
        bytes_sent: u64,
        bytes_received: u64,
        active_streams: u32,
        updated_at: DateTime<Utc>,
    },
    
    /// Error occurred
    Error {
        error: String,
        context: Option<String>,
        occurred_at: DateTime<Utc>,
    },
    
    /// Warning issued
    Warning {
        message: String,
        context: Option<String>,
        issued_at: DateTime<Utc>,
    },
    
    /// Custom application event
    Custom {
        event_type: String,
        data: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
}

/// Event severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventSeverity {
    /// Informational event
    Info,
    /// Warning event
    Warning,
    /// Error event
    Error,
    /// Critical event
    Critical,
}

/// Event category for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventCategory {
    /// Daemon-related events
    Daemon,
    /// Stream-related events
    Stream,
    /// Network-related events
    Network,
    /// Configuration-related events
    Config,
    /// Metrics-related events
    Metrics,
    /// Application-defined events
    Custom,
}

impl NyxEvent {
    /// Get the event severity
    pub fn severity(&self) -> EventSeverity {
        match self {
            NyxEvent::DaemonConnected { .. } => EventSeverity::Info,
            NyxEvent::DaemonDisconnected { .. } => EventSeverity::Warning,
            NyxEvent::DaemonStatusChanged { new_status, .. } => {
                match new_status {
                    ConnectionStatus::Connected => EventSeverity::Info,
                    ConnectionStatus::Degraded => EventSeverity::Warning,
                    ConnectionStatus::Reconnecting => EventSeverity::Warning,
                    ConnectionStatus::Disconnected => EventSeverity::Error,
                    ConnectionStatus::Failed => EventSeverity::Critical,
                }
            }
            NyxEvent::DaemonHealthChanged { new_health, .. } => {
                match new_health {
                    HealthStatus::Healthy => EventSeverity::Info,
                    HealthStatus::Degraded => EventSeverity::Warning,
                    HealthStatus::Critical => EventSeverity::Critical,
                    HealthStatus::Unknown => EventSeverity::Warning,
                }
            }
            NyxEvent::StreamOpened { .. } => EventSeverity::Info,
            NyxEvent::StreamClosed { .. } => EventSeverity::Info,
            NyxEvent::StreamError { .. } => EventSeverity::Error,
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnecting { .. } => EventSeverity::Warning,
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnected { .. } => EventSeverity::Info,
            NyxEvent::ConfigChanged { .. } => EventSeverity::Info,
            NyxEvent::NetworkPathAdded { .. } => EventSeverity::Info,
            NyxEvent::NetworkPathRemoved { .. } => EventSeverity::Warning,
            NyxEvent::MetricsUpdated { .. } => EventSeverity::Info,
            NyxEvent::Error { .. } => EventSeverity::Error,
            NyxEvent::Warning { .. } => EventSeverity::Warning,
            NyxEvent::Custom { .. } => EventSeverity::Info,
        }
    }
    
    /// Get the event category
    pub fn category(&self) -> EventCategory {
        match self {
            NyxEvent::DaemonConnected { .. } |
            NyxEvent::DaemonDisconnected { .. } |
            NyxEvent::DaemonStatusChanged { .. } |
            NyxEvent::DaemonHealthChanged { .. } => EventCategory::Daemon,
            
            NyxEvent::StreamOpened { .. } |
            NyxEvent::StreamClosed { .. } |
            NyxEvent::StreamError { .. } => EventCategory::Stream,
            
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnecting { .. } => EventCategory::Stream,
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnected { .. } => EventCategory::Stream,
            
            NyxEvent::NetworkPathAdded { .. } |
            NyxEvent::NetworkPathRemoved { .. } => EventCategory::Network,
            
            NyxEvent::ConfigChanged { .. } => EventCategory::Config,
            
            NyxEvent::MetricsUpdated { .. } => EventCategory::Metrics,
            
            NyxEvent::Error { .. } |
            NyxEvent::Warning { .. } => EventCategory::Custom,
            
            NyxEvent::Custom { .. } => EventCategory::Custom,
        }
    }
    
    /// Get the event timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            NyxEvent::DaemonConnected { connected_at, .. } => *connected_at,
            NyxEvent::DaemonDisconnected { disconnected_at, .. } => *disconnected_at,
            NyxEvent::DaemonStatusChanged { changed_at, .. } => *changed_at,
            NyxEvent::DaemonHealthChanged { changed_at, .. } => *changed_at,
            NyxEvent::StreamError { error_at, .. } => *error_at,
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnecting { started_at, .. } => *started_at,
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnected { reconnected_at, .. } => *reconnected_at,
            NyxEvent::ConfigChanged { changed_at, .. } => *changed_at,
            NyxEvent::NetworkPathAdded { added_at, .. } => *added_at,
            NyxEvent::NetworkPathRemoved { removed_at, .. } => *removed_at,
            NyxEvent::MetricsUpdated { updated_at, .. } => *updated_at,
            NyxEvent::Error { occurred_at, .. } => *occurred_at,
            NyxEvent::Warning { issued_at, .. } => *issued_at,
            NyxEvent::Custom { timestamp, .. } => *timestamp,
            _ => Utc::now(), // For events without explicit timestamps
        }
    }
    
    /// Create a custom event
    pub fn custom(event_type: impl Into<String>, data: serde_json::Value) -> Self {
        Self::Custom {
            event_type: event_type.into(),
            data,
            timestamp: Utc::now(),
        }
    }
    
    /// Create an error event
    pub fn error(error: impl Into<String>, context: Option<String>) -> Self {
        Self::Error {
            error: error.into(),
            context,
            occurred_at: Utc::now(),
        }
    }
    
    /// Create a warning event
    pub fn warning(message: impl Into<String>, context: Option<String>) -> Self {
        Self::Warning {
            message: message.into(),
            context,
            issued_at: Utc::now(),
        }
    }
}

impl fmt::Display for NyxEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NyxEvent::DaemonConnected { node_id, version, .. } => {
                write!(f, "Daemon connected: {} (version {})", node_id, version)
            }
            NyxEvent::DaemonDisconnected { reason, .. } => {
                write!(f, "Daemon disconnected: {}", reason)
            }
            NyxEvent::DaemonStatusChanged { old_status, new_status, .. } => {
                write!(f, "Daemon status changed: {:?} → {:?}", old_status, new_status)
            }
            NyxEvent::DaemonHealthChanged { old_health, new_health, .. } => {
                write!(f, "Daemon health changed: {:?} → {:?}", old_health, new_health)
            }
            NyxEvent::StreamOpened { stream_id, target } => {
                write!(f, "Stream {} opened to {}", stream_id, target)
            }
            NyxEvent::StreamClosed { stream_id } => {
                write!(f, "Stream {} closed", stream_id)
            }
            NyxEvent::StreamError { stream_id, error, .. } => {
                write!(f, "Stream {} error: {}", stream_id, error)
            }
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnecting { stream_id, attempt, .. } => {
                write!(f, "Stream {} reconnecting (attempt {})", stream_id, attempt)
            }
            #[cfg(feature = "reconnect")]
            NyxEvent::StreamReconnected { old_stream_id, new_stream_id, .. } => {
                write!(f, "Stream {} reconnected as {}", old_stream_id, new_stream_id)
            }
            NyxEvent::ConfigChanged { .. } => {
                write!(f, "Configuration changed")
            }
            NyxEvent::NetworkPathAdded { path_id, endpoint, .. } => {
                write!(f, "Network path {} added: {}", path_id, endpoint)
            }
            NyxEvent::NetworkPathRemoved { path_id, endpoint, .. } => {
                write!(f, "Network path {} removed: {}", path_id, endpoint)
            }
            NyxEvent::MetricsUpdated { active_streams, .. } => {
                write!(f, "Metrics updated: {} active streams", active_streams)
            }
            NyxEvent::Error { error, context, .. } => {
                match context {
                    Some(ctx) => write!(f, "Error in {}: {}", ctx, error),
                    None => write!(f, "Error: {}", error),
                }
            }
            NyxEvent::Warning { message, context, .. } => {
                match context {
                    Some(ctx) => write!(f, "Warning in {}: {}", ctx, message),
                    None => write!(f, "Warning: {}", message),
                }
            }
            NyxEvent::Custom { event_type, .. } => {
                write!(f, "Custom event: {}", event_type)
            }
        }
    }
}

/// Trait for handling SDK events
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event
    async fn handle_event(&self, event: NyxEvent);
    
    /// Check if this handler is interested in the given event
    fn should_handle(&self, _event: &NyxEvent) -> bool {
        // Default: handle all events
        true
    }
    
    /// Get the name of this handler (for debugging)
    fn name(&self) -> &str {
        "EventHandler"
    }
}

/// Event filter for selective event handling
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// Minimum severity level to handle
    pub min_severity: Option<EventSeverity>,
    /// Categories to include (None = all categories)
    pub categories: Option<Vec<EventCategory>>,
    /// Specific event types to include
    pub event_types: Option<Vec<String>>,
}

impl EventFilter {
    /// Create a new event filter
    pub fn new() -> Self {
        Self {
            min_severity: None,
            categories: None,
            event_types: None,
        }
    }
    
    /// Set minimum severity level
    pub fn min_severity(mut self, severity: EventSeverity) -> Self {
        self.min_severity = Some(severity);
        self
    }
    
    /// Set categories to include
    pub fn categories(mut self, categories: Vec<EventCategory>) -> Self {
        self.categories = Some(categories);
        self
    }
    
    /// Set specific event types to include
    pub fn event_types(mut self, types: Vec<String>) -> Self {
        self.event_types = Some(types);
        self
    }
    
    /// Check if an event passes this filter
    pub fn matches(&self, event: &NyxEvent) -> bool {
        // Check severity
        if let Some(min_severity) = self.min_severity {
            let event_severity = event.severity();
            let severity_ok = match (min_severity, event_severity) {
                (EventSeverity::Info, _) => true,
                (EventSeverity::Warning, EventSeverity::Warning | EventSeverity::Error | EventSeverity::Critical) => true,
                (EventSeverity::Error, EventSeverity::Error | EventSeverity::Critical) => true,
                (EventSeverity::Critical, EventSeverity::Critical) => true,
                _ => false,
            };
            if !severity_ok {
                return false;
            }
        }
        
        // Check category
        if let Some(ref categories) = self.categories {
            if !categories.contains(&event.category()) {
                return false;
            }
        }
        
        // Check specific event types
        if let Some(ref event_types) = self.event_types {
            let event_type = match event {
                NyxEvent::Custom { event_type, .. } => event_type.as_str(),
                _ => return true, // Non-custom events pass type filter
            };
            if !event_types.iter().any(|t| t == event_type) {
                return false;
            }
        }
        
        true
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Filtered event handler that applies a filter before handling events
pub struct FilteredEventHandler {
    handler: Arc<dyn EventHandler>,
    filter: EventFilter,
}

impl FilteredEventHandler {
    /// Create a new filtered event handler
    pub fn new(handler: Arc<dyn EventHandler>, filter: EventFilter) -> Self {
        Self { handler, filter }
    }
}

#[async_trait]
impl EventHandler for FilteredEventHandler {
    async fn handle_event(&self, event: NyxEvent) {
        if self.filter.matches(&event) {
            self.handler.handle_event(event).await;
        }
    }
    
    fn should_handle(&self, event: &NyxEvent) -> bool {
        self.filter.matches(event) && self.handler.should_handle(event)
    }
    
    fn name(&self) -> &str {
        self.handler.name()
    }
}

/// Simple logging event handler
pub struct LoggingEventHandler {
    name: String,
}

impl LoggingEventHandler {
    /// Create a new logging event handler
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
        }
    }
}

#[async_trait]
impl EventHandler for LoggingEventHandler {
    async fn handle_event(&self, event: NyxEvent) {
        match event.severity() {
            EventSeverity::Info => tracing::info!("{}", event),
            EventSeverity::Warning => tracing::warn!("{}", event),
            EventSeverity::Error => tracing::error!("{}", event),
            EventSeverity::Critical => tracing::error!("{}", event),
        };
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Callback-based event handler
pub struct CallbackEventHandler {
    name: String,
    callback: EventCallback,
}

/// Type alias for event callback functions
pub type EventCallback = Box<dyn Fn(NyxEvent) + Send + Sync>;

impl CallbackEventHandler {
    /// Create a new callback event handler
    pub fn new(name: impl Into<String>, callback: EventCallback) -> Self {
        Self {
            name: name.into(),
            callback,
        }
    }
}

#[async_trait]
impl EventHandler for CallbackEventHandler {
    async fn handle_event(&self, event: NyxEvent) {
        (self.callback)(event);
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Event manager for handling multiple event handlers
pub struct EventManager {
    handlers: Arc<tokio::sync::RwLock<Vec<Arc<dyn EventHandler>>>>,
    event_queue: Arc<tokio::sync::Mutex<Vec<NyxEvent>>>,
    stats: Arc<tokio::sync::RwLock<EventStats>>,
}

/// Event statistics
#[derive(Debug, Clone, Default)]
pub struct EventStats {
    pub total_events: u64,
    pub events_by_category: std::collections::HashMap<EventCategory, u64>,
    pub events_by_severity: std::collections::HashMap<EventSeverity, u64>,
    pub handlers_count: usize,
    pub last_event_time: Option<DateTime<Utc>>,
}

impl EventManager {
    /// Create a new event manager
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            event_queue: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            stats: Arc::new(tokio::sync::RwLock::new(EventStats::default())),
        }
    }
    
    /// Add an event handler
    pub async fn add_handler(&self, handler: Arc<dyn EventHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.push(handler);
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.handlers_count = handlers.len();
    }
    
    /// Remove an event handler by name
    pub async fn remove_handler(&self, name: &str) -> bool {
        let mut handlers = self.handlers.write().await;
        let initial_len = handlers.len();
        handlers.retain(|h| h.name() != name);
        
        let removed = handlers.len() != initial_len;
        if removed {
            // Update stats
            let mut stats = self.stats.write().await;
            stats.handlers_count = handlers.len();
        }
        
        removed
    }
    
    /// Emit an event to all handlers
    pub async fn emit(&self, event: NyxEvent) {
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_events += 1;
            stats.last_event_time = Some(event.timestamp());
            
            // Update category stats
            *stats.events_by_category.entry(event.category()).or_insert(0) += 1;
            
            // Update severity stats
            *stats.events_by_severity.entry(event.severity()).or_insert(0) += 1;
        }
        
        // Send to all handlers
        let handlers = self.handlers.read().await;
        let mut tasks = Vec::new();
        
        for handler in handlers.iter() {
            if handler.should_handle(&event) {
                let handler = Arc::clone(handler);
                let event = event.clone();
                
                let task = tokio::spawn(async move {
                    handler.handle_event(event).await;
                });
                
                tasks.push(task);
            }
        }
        
        // Wait for all handlers to complete
        for task in tasks {
            let _ = task.await;
        }
    }
    
    /// Get event statistics
    pub async fn stats(&self) -> EventStats {
        self.stats.read().await.clone()
    }
    
    /// Clear all handlers
    pub async fn clear_handlers(&self) {
        let mut handlers = self.handlers.write().await;
        handlers.clear();
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.handlers_count = 0;
    }
    
    /// Get the number of registered handlers
    pub async fn handler_count(&self) -> usize {
        self.handlers.read().await.len()
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
} 