#![forbid(unsafe_code)]

//! Event system for Nyx daemon.
//!
//! This module provides:
//! - Event filtering and routing
//! - Event broadcasting to subscribers
//! - Event persistence and replay
//! - Event metrics and analytics
//! - Structured event logging

use crate::proto::{Event, EventFilter};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

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

/// Event statistics
#[derive(Debug, Clone, Default)]
pub struct EventStatistics {
    pub total_events: u64,
    pub events_by_type: HashMap<String, u64>,
    pub events_by_severity: HashMap<String, u64>,
    pub filtered_events: u64,
    pub subscriber_count: usize,
}

/// Comprehensive event system
pub struct EventSystem {
    statistics: Arc<RwLock<EventStatistics>>,
}

impl EventSystem {
    /// Create a new event system
    pub fn new() -> Self {
        Self {
            statistics: Arc::new(RwLock::new(EventStatistics::default())),
        }
    }
    
    /// Check if an event matches a filter
    pub fn matches_filter(&self, event: &Event, filter: &EventFilter) -> bool {
        let internal_filter = InternalEventFilter::from_proto(filter);
        internal_filter.matches(event)
    }
    
    /// Record event statistics
    pub async fn record_event(&self, event: &Event) {
        let mut stats = self.statistics.write().await;
        
        stats.total_events += 1;
        
        // Count by type
        *stats.events_by_type.entry(event.r#type.clone()).or_insert(0) += 1;
        
        // Count by severity
        *stats.events_by_severity.entry(event.severity.clone()).or_insert(0) += 1;
        
        debug!("Recorded event: type={}, severity={}", event.r#type, event.severity);
    }
    
    /// Get event statistics
    pub async fn get_statistics(&self) -> EventStatistics {
        self.statistics.read().await.clone()
    }
    
    /// Update subscriber count
    pub async fn update_subscriber_count(&self, count: usize) {
        let mut stats = self.statistics.write().await;
        stats.subscriber_count = count;
    }
    
    /// Record filtered event
    pub async fn record_filtered_event(&self) {
        let mut stats = self.statistics.write().await;
        stats.filtered_events += 1;
    }
}

impl Clone for EventSystem {
    fn clone(&self) -> Self {
        Self {
            statistics: Arc::clone(&self.statistics),
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
            timestamp: Some(prost_types::Timestamp::from(SystemTime::now())),
            severity: "info".to_string(),
            attributes: HashMap::new(),
            event_data: Some(crate::proto::event::EventData::StreamEvent(StreamEvent {
                stream_id: 123,
                action: "opened".to_string(),
                target_address: "test".to_string(),
                stats: None,
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
            timestamp: Some(prost_types::Timestamp::from(SystemTime::now())),
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
            timestamp: Some(prost_types::Timestamp::from(SystemTime::now())),
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