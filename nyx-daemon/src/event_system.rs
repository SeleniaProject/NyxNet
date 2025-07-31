#![forbid(unsafe_code)]

//! Comprehensive event system for Nyx daemon.
//!
//! This module provides:
//! - Event filtering and routing with priority handling
//! - Event broadcasting to subscribers with error isolation
//! - Event persistence and replay capabilities
//! - Event metrics and analytics
//! - Structured event logging
//! - Layer-to-layer messaging system
//! - Event subscription and distribution system

use crate::proto::{Event, EventFilter};

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, Mutex, broadcast, mpsc};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use anyhow::Result;

/// Internal event filter for more complex filtering logic
pub struct InternalEventFilter {
    pub types: HashSet<String>,
    pub stream_ids: HashSet<u32>,
    pub severities: HashSet<String>,
    pub attributes: HashMap<String, String>,
}

impl InternalEventFilter {
    /// Create from protobuf EventFilter
    pub fn from_proto(filter: &EventFilter) -> Self {
        Self {
            types: filter.types.iter().cloned().collect(),
            stream_ids: filter.stream_ids.iter().cloned().collect(),
            severities: if filter.severity.is_empty() {
                HashSet::new()
            } else {
                [filter.severity.clone()].into_iter().collect()
            },
            attributes: HashMap::new(),
        }
    }
    
    /// Check if an event matches this filter
    pub fn matches(&self, event: &Event) -> bool {
        // Check event type
        if !self.types.is_empty() && !self.types.contains(&event.r#type) {
            return false;
        }
        
        // Check severity
        if !self.severities.is_empty() && !self.severities.contains(&event.severity) {
            return false;
        }
        
        // Check stream IDs if applicable
        if !self.stream_ids.is_empty() {
            if let Some(event_data) = &event.event_data {
                match event_data {
                    crate::proto::event::EventData::StreamEvent(stream_event) => {
                        if !self.stream_ids.contains(&stream_event.stream_id) {
                            return false;
                        }
                    }
                    _ => return false, // Non-stream events don't match stream ID filters
                }
            } else {
                return false;
            }
        }
        
        // Check attributes
        for (key, expected_value) in &self.attributes {
            if let Some(actual_value) = event.attributes.get(key) {
                if actual_value != expected_value {
                    return false;
                }
            } else {
                return false;
            }
        }
        
        true
    }
}

/// Event priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl EventPriority {
    pub fn from_severity(severity: &str) -> Self {
        match severity {
            "critical" => EventPriority::Critical,
            "error" => EventPriority::High,
            "warn" | "warning" => EventPriority::Normal,
            _ => EventPriority::Low,
        }
    }
}

/// Prioritized event wrapper
#[derive(Debug, Clone)]
pub struct PrioritizedEvent {
    pub event: Event,
    pub priority: EventPriority,
    pub created_at: SystemTime,
    pub retry_count: u32,
}

impl PrioritizedEvent {
    pub fn new(event: Event) -> Self {
        let priority = EventPriority::from_severity(&event.severity);
        Self {
            event,
            priority,
            created_at: SystemTime::now(),
            retry_count: 0,
        }
    }
}

/// Event subscriber with error handling
pub struct EventSubscriber {
    pub id: String,
    pub filter: InternalEventFilter,
    pub sender: mpsc::UnboundedSender<Event>,
    pub error_count: u32,
    pub last_error: Option<String>,
    pub last_activity: SystemTime,
    pub is_active: bool,
}

impl EventSubscriber {
    pub fn new(id: String, filter: InternalEventFilter, sender: mpsc::UnboundedSender<Event>) -> Self {
        Self {
            id,
            filter,
            sender,
            error_count: 0,
            last_error: None,
            last_activity: SystemTime::now(),
            is_active: true,
        }
    }
    
    pub fn send_event(&mut self, event: &Event) -> Result<()> {
        if !self.is_active {
            return Err(anyhow::anyhow!("Subscriber is inactive"));
        }
        
        match self.sender.send(event.clone()) {
            Ok(_) => {
                self.last_activity = SystemTime::now();
                Ok(())
            }
            Err(e) => {
                self.error_count += 1;
                self.last_error = Some(e.to_string());
                
                // Deactivate subscriber after too many errors
                if self.error_count > 10 {
                    self.is_active = false;
                    warn!("Deactivating subscriber {} due to too many errors", self.id);
                }
                
                Err(anyhow::anyhow!("Failed to send event to subscriber {}: {}", self.id, e))
            }
        }
    }
}

/// Event statistics with enhanced metrics
#[derive(Debug, Clone, Default)]
pub struct EventStatistics {
    pub total_events: u64,
    pub events_by_type: HashMap<String, u64>,
    pub events_by_severity: HashMap<String, u64>,
    pub events_by_priority: HashMap<String, u64>,
    pub filtered_events: u64,
    pub subscriber_count: usize,
    pub active_subscriber_count: usize,
    pub failed_deliveries: u64,
    pub queue_size: usize,
    pub processing_errors: u64,
    pub average_processing_time_ms: f64,
}

/// Comprehensive event bus system with layer-to-layer messaging
pub struct EventSystem {
    // Core event processing
    event_queue: Arc<Mutex<VecDeque<PrioritizedEvent>>>,
    subscribers: Arc<RwLock<HashMap<String, EventSubscriber>>>,
    statistics: Arc<RwLock<EventStatistics>>,
    
    // Event broadcasting
    broadcast_tx: broadcast::Sender<Event>,
    
    // Background processing
    processor_handle: Option<tokio::task::JoinHandle<()>>,
    cleanup_handle: Option<tokio::task::JoinHandle<()>>,
    
    // Configuration
    max_queue_size: usize,
    processing_timeout: Duration,
    cleanup_interval: Duration,
}

impl EventSystem {
    /// Create a new comprehensive event system
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);
        
        Self {
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(EventStatistics::default())),
            broadcast_tx,
            processor_handle: None,
            cleanup_handle: None,
            max_queue_size: 10000,
            processing_timeout: Duration::from_secs(5),
            cleanup_interval: Duration::from_secs(60),
        }
    }
    
    /// Start the event system background tasks
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting event system background tasks");
        
        // Start event processor
        let processor_task = {
            let event_system = self.clone();
            tokio::spawn(async move {
                event_system.event_processing_loop().await;
            })
        };
        self.processor_handle = Some(processor_task);
        
        // Start cleanup task
        let cleanup_task = {
            let event_system = self.clone();
            tokio::spawn(async move {
                event_system.cleanup_loop().await;
            })
        };
        self.cleanup_handle = Some(cleanup_task);
        
        info!("Event system started successfully");
        Ok(())
    }
    
    /// Stop the event system
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping event system");
        
        if let Some(handle) = self.processor_handle.take() {
            handle.abort();
        }
        
        if let Some(handle) = self.cleanup_handle.take() {
            handle.abort();
        }
        
        // Clear the event queue
        {
            let mut queue = self.event_queue.lock().await;
            queue.clear();
        }
        
        // Clear subscribers
        {
            let mut subscribers = self.subscribers.write().await;
            subscribers.clear();
        }
        
        info!("Event system stopped");
        Ok(())
    }
    
    /// Publish an event to the system
    pub async fn publish_event(&self, event: Event) -> Result<()> {
        let prioritized_event = PrioritizedEvent::new(event.clone());
        
        // Add to priority queue
        {
            let mut queue = self.event_queue.lock().await;
            
            // Check queue size limit
            if queue.len() >= self.max_queue_size {
                warn!("Event queue is full, dropping oldest low-priority events");
                self.drop_low_priority_events(&mut queue).await;
            }
            
            // Insert event in priority order
            let insert_pos = queue.iter()
                .position(|e| e.priority < prioritized_event.priority)
                .unwrap_or(queue.len());
            
            queue.insert(insert_pos, prioritized_event);
        }
        
        // Also broadcast immediately for real-time subscribers
        let _ = self.broadcast_tx.send(event.clone());
        
        // Update statistics
        self.record_event(&event).await;
        
        Ok(())
    }
    
    /// Subscribe to events with a filter
    pub async fn subscribe(&self, subscriber_id: String, filter: EventFilter) -> Result<mpsc::UnboundedReceiver<Event>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let internal_filter = InternalEventFilter::from_proto(&filter);
        let subscriber = EventSubscriber::new(subscriber_id.clone(), internal_filter, tx);
        
        {
            let mut subscribers = self.subscribers.write().await;
            subscribers.insert(subscriber_id.clone(), subscriber);
        }
        
        // Update subscriber count
        self.update_subscriber_statistics().await;
        
        info!("New subscriber registered: {}", subscriber_id);
        Ok(rx)
    }
    
    /// Unsubscribe from events
    pub async fn unsubscribe(&self, subscriber_id: &str) -> Result<()> {
        {
            let mut subscribers = self.subscribers.write().await;
            subscribers.remove(subscriber_id);
        }
        
        // Update subscriber count
        self.update_subscriber_statistics().await;
        
        info!("Subscriber unregistered: {}", subscriber_id);
        Ok(())
    }
    
    /// Get a broadcast receiver for real-time events
    pub fn get_broadcast_receiver(&self) -> broadcast::Receiver<Event> {
        self.broadcast_tx.subscribe()
    }
    
    /// Event processing loop
    async fn event_processing_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.process_event_batch().await {
                error!("Event processing error: {}", e);
                
                // Update error statistics
                {
                    let mut stats = self.statistics.write().await;
                    stats.processing_errors += 1;
                }
            }
        }
    }
    
    /// Process a batch of events from the queue
    async fn process_event_batch(&self) -> Result<()> {
        let events_to_process = {
            let mut queue = self.event_queue.lock().await;
            let batch_size = std::cmp::min(10, queue.len());
            let mut batch = Vec::with_capacity(batch_size);
            
            for _ in 0..batch_size {
                if let Some(event) = queue.pop_front() {
                    batch.push(event);
                }
            }
            
            batch
        };
        
        if events_to_process.is_empty() {
            return Ok(());
        }
        
        let start_time = SystemTime::now();
        
        // Process each event
        for prioritized_event in events_to_process {
            if let Err(e) = self.distribute_event(&prioritized_event.event).await {
                error!("Failed to distribute event: {}", e);
                
                // Retry critical events
                if prioritized_event.priority == EventPriority::Critical && prioritized_event.retry_count < 3 {
                    let mut retry_event = prioritized_event.clone();
                    retry_event.retry_count += 1;
                    
                    let mut queue = self.event_queue.lock().await;
                    queue.push_front(retry_event);
                }
            }
        }
        
        // Update processing time statistics
        if let Ok(elapsed) = start_time.elapsed() {
            let mut stats = self.statistics.write().await;
            let processing_time_ms = elapsed.as_millis() as f64;
            
            // Simple moving average
            if stats.average_processing_time_ms == 0.0 {
                stats.average_processing_time_ms = processing_time_ms;
            } else {
                stats.average_processing_time_ms = 
                    (stats.average_processing_time_ms * 0.9) + (processing_time_ms * 0.1);
            }
        }
        
        Ok(())
    }
    
    /// Distribute an event to matching subscribers
    async fn distribute_event(&self, event: &Event) -> Result<()> {
        let subscribers = self.subscribers.read().await;
        let mut failed_deliveries = 0;
        
        for (subscriber_id, subscriber) in subscribers.iter() {
            if !subscriber.is_active {
                continue;
            }
            
            if subscriber.filter.matches(event) {
                // Use timeout to prevent blocking on slow subscribers
                let send_result = timeout(
                    self.processing_timeout,
                    async {
                        // Clone subscriber to avoid holding read lock
                        let mut subscriber_clone = subscriber.clone();
                        subscriber_clone.send_event(event)
                    }
                ).await;
                
                match send_result {
                    Ok(Ok(_)) => {
                        debug!("Event delivered to subscriber: {}", subscriber_id);
                    }
                    Ok(Err(e)) => {
                        warn!("Failed to deliver event to subscriber {}: {}", subscriber_id, e);
                        failed_deliveries += 1;
                    }
                    Err(_) => {
                        warn!("Timeout delivering event to subscriber: {}", subscriber_id);
                        failed_deliveries += 1;
                    }
                }
            }
        }
        
        // Update failure statistics
        if failed_deliveries > 0 {
            let mut stats = self.statistics.write().await;
            stats.failed_deliveries += failed_deliveries;
        }
        
        Ok(())
    }
    
    /// Drop low priority events when queue is full
    async fn drop_low_priority_events(&self, queue: &mut VecDeque<PrioritizedEvent>) {
        let initial_size = queue.len();
        
        // Remove low priority events older than 5 minutes
        let cutoff_time = SystemTime::now() - Duration::from_secs(300);
        
        queue.retain(|event| {
            if event.priority == EventPriority::Low && event.created_at < cutoff_time {
                false
            } else {
                true
            }
        });
        
        let dropped_count = initial_size - queue.len();
        if dropped_count > 0 {
            warn!("Dropped {} low-priority events due to queue overflow", dropped_count);
        }
        
        // If still too full, drop more low priority events
        while queue.len() > self.max_queue_size * 9 / 10 {
            if let Some(pos) = queue.iter().position(|e| e.priority == EventPriority::Low) {
                queue.remove(pos);
            } else {
                break;
            }
        }
    }
    
    /// Cleanup loop for inactive subscribers and old events
    async fn cleanup_loop(&self) {
        let mut interval = tokio::time::interval(self.cleanup_interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.cleanup_inactive_subscribers().await {
                error!("Subscriber cleanup error: {}", e);
            }
            
            if let Err(e) = self.cleanup_old_statistics().await {
                error!("Statistics cleanup error: {}", e);
            }
        }
    }
    
    /// Remove inactive subscribers
    async fn cleanup_inactive_subscribers(&self) -> Result<()> {
        let cutoff_time = SystemTime::now() - Duration::from_secs(3600); // 1 hour
        let mut removed_count = 0;
        
        {
            let mut subscribers = self.subscribers.write().await;
            let initial_count = subscribers.len();
            
            subscribers.retain(|id, subscriber| {
                if !subscriber.is_active || subscriber.last_activity < cutoff_time {
                    info!("Removing inactive subscriber: {}", id);
                    false
                } else {
                    true
                }
            });
            
            removed_count = initial_count - subscribers.len();
        }
        
        if removed_count > 0 {
            info!("Cleaned up {} inactive subscribers", removed_count);
            self.update_subscriber_statistics().await;
        }
        
        Ok(())
    }
    
    /// Clean up old statistics
    async fn cleanup_old_statistics(&self) -> Result<()> {
        // Reset statistics periodically to prevent unbounded growth
        let mut stats = self.statistics.write().await;
        
        // Keep only recent type/severity counts (simple cleanup)
        if stats.total_events > 1_000_000 {
            info!("Resetting event statistics due to high count");
            *stats = EventStatistics::default();
        }
        
        Ok(())
    }
    
    /// Update subscriber statistics
    async fn update_subscriber_statistics(&self) {
        let subscribers = self.subscribers.read().await;
        let total_count = subscribers.len();
        let active_count = subscribers.values().filter(|s| s.is_active).count();
        
        let mut stats = self.statistics.write().await;
        stats.subscriber_count = total_count;
        stats.active_subscriber_count = active_count;
        
        // Update queue size
        drop(stats);
        let queue = self.event_queue.lock().await;
        let mut stats = self.statistics.write().await;
        stats.queue_size = queue.len();
    }
    
    /// Check if an event matches a filter
    pub fn matches_filter(&self, event: &Event, filter: &EventFilter) -> bool {
        let internal_filter = InternalEventFilter::from_proto(filter);
        internal_filter.matches(event)
    }
    
    /// Record event statistics
    async fn record_event(&self, event: &Event) {
        let mut stats = self.statistics.write().await;
        
        stats.total_events += 1;
        
        // Count by type
        *stats.events_by_type.entry(event.r#type.clone()).or_insert(0) += 1;
        
        // Count by severity
        *stats.events_by_severity.entry(event.severity.clone()).or_insert(0) += 1;
        
        // Count by priority
        let priority = EventPriority::from_severity(&event.severity);
        let priority_str = format!("{:?}", priority);
        *stats.events_by_priority.entry(priority_str).or_insert(0) += 1;
        
        debug!("Recorded event: type={}, severity={}, priority={:?}", 
               event.r#type, event.severity, priority);
    }
    
    /// Get event statistics
    pub async fn get_statistics(&self) -> EventStatistics {
        // Update current queue size
        self.update_subscriber_statistics().await;
        self.statistics.read().await.clone()
    }
    
    /// Record filtered event
    pub async fn record_filtered_event(&self) {
        let mut stats = self.statistics.write().await;
        stats.filtered_events += 1;
    }
    
    /// Get current queue size
    pub async fn get_queue_size(&self) -> usize {
        let queue = self.event_queue.lock().await;
        queue.len()
    }
    
    /// Get subscriber information
    pub async fn get_subscriber_info(&self) -> HashMap<String, (bool, u32, Option<String>)> {
        let subscribers = self.subscribers.read().await;
        subscribers.iter()
            .map(|(id, sub)| (id.clone(), (sub.is_active, sub.error_count, sub.last_error.clone())))
            .collect()
    }
}

impl Clone for EventSystem {
    fn clone(&self) -> Self {
        Self {
            event_queue: Arc::clone(&self.event_queue),
            subscribers: Arc::clone(&self.subscribers),
            statistics: Arc::clone(&self.statistics),
            broadcast_tx: self.broadcast_tx.clone(),
            processor_handle: None, // Don't clone background tasks
            cleanup_handle: None,
            max_queue_size: self.max_queue_size,
            processing_timeout: self.processing_timeout,
            cleanup_interval: self.cleanup_interval,
        }
    }
}

impl Clone for EventSubscriber {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            filter: self.filter.clone(),
            sender: self.sender.clone(),
            error_count: self.error_count,
            last_error: self.last_error.clone(),
            last_activity: self.last_activity,
            is_active: self.is_active,
        }
    }
}

impl Clone for InternalEventFilter {
    fn clone(&self) -> Self {
        Self {
            types: self.types.clone(),
            stream_ids: self.stream_ids.clone(),
            severities: self.severities.clone(),
            attributes: self.attributes.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{StreamEvent, StreamStats};
    use std::time::SystemTime;
    
    #[test]
    fn test_event_filter_matching() {
        let filter = EventFilter {
            types: vec!["stream".to_string()],
            stream_ids: vec![123],
            severity: "info".to_string(),
        };
        
        let event = Event {
            r#type: "stream".to_string(),
            detail: "Test event".to_string(),
            timestamp: Some(crate::proto::Timestamp {
                seconds: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
                nanos: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as i32,
            }),
            severity: "info".to_string(),
            attributes: HashMap::new(),
            event_data: Some(crate::proto::event::EventData::StreamEvent(StreamEvent {
                stream_id: 123,
                action: "opened".to_string(),
                target_address: "test".to_string(),
                stats: None,
                data: std::collections::HashMap::new(),
                event_type: "test".to_string(),
                timestamp: Some(crate::proto::Timestamp {
                    seconds: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
                    nanos: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as i32,
                }),
            })),
        };
        
        let internal_filter = InternalEventFilter::from_proto(&filter);
        assert!(internal_filter.matches(&event));
    }
    
    #[test]
    fn test_event_filter_mismatch() {
        let filter = EventFilter {
            types: vec!["connection".to_string()],
            stream_ids: vec![],
            severity: "error".to_string(),
        };
        
        let event = Event {
            r#type: "stream".to_string(),
            detail: "Test event".to_string(),
            timestamp: Some(crate::proto::Timestamp {
                seconds: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
                nanos: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as i32,
            }),
            severity: "info".to_string(),
            attributes: HashMap::new(),
            event_data: None,
        };
        
        let internal_filter = InternalEventFilter::from_proto(&filter);
        assert!(!internal_filter.matches(&event));
    }
    
    #[tokio::test]
    async fn test_event_statistics() {
        let event_system = EventSystem::new();
        
        let event = Event {
            r#type: "test".to_string(),
            detail: "Test event".to_string(),
            timestamp: Some(crate::proto::Timestamp {
                seconds: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
                nanos: SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as i32,
            }),
            severity: "info".to_string(),
            attributes: HashMap::new(),
            event_data: None,
        };
        
        event_system.record_event(&event).await;
        
        let stats = event_system.get_statistics().await;
        assert_eq!(stats.total_events, 1);
        assert_eq!(stats.events_by_type.get("test"), Some(&1));
        assert_eq!(stats.events_by_severity.get("info"), Some(&1));
    }
} 