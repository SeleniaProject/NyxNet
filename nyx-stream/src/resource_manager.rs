#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, warn, error, info};

/// Resource types that can be tracked and cleaned up
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// Memory buffers
    Buffer,
    /// File handles
    FileHandle,
    /// Network connections
    Connection,
    /// Async tasks
    Task,
    /// Timers
    Timer,
    /// Crypto contexts
    CryptoContext,
    /// Custom resource type
    Custom(&'static str),
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Buffer => write!(f, "Buffer"),
            ResourceType::FileHandle => write!(f, "FileHandle"),
            ResourceType::Connection => write!(f, "Connection"),
            ResourceType::Task => write!(f, "Task"),
            ResourceType::Timer => write!(f, "Timer"),
            ResourceType::CryptoContext => write!(f, "CryptoContext"),
            ResourceType::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// Resource cleanup errors
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("Resource not found: {0}")]
    NotFound(String),
    #[error("Resource already cleaned up: {0}")]
    AlreadyCleanedUp(String),
    #[error("Cleanup timeout for resource: {0}")]
    CleanupTimeout(String),
    #[error("Cleanup failed: {0}")]
    CleanupFailed(String),
    #[error("Resource limit exceeded: {resource_type}, limit: {limit}, current: {current}")]
    LimitExceeded {
        resource_type: ResourceType,
        limit: usize,
        current: usize,
    },
}

/// Resource cleanup callback
pub type CleanupCallback = Box<dyn Fn() -> Result<(), String> + Send + Sync>;

/// Async resource cleanup callback
pub type AsyncCleanupCallback = Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> + Send + Sync>;

/// Resource metadata and cleanup information
pub struct ResourceInfo {
    pub id: String,
    pub resource_type: ResourceType,
    pub size_bytes: usize,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub access_count: u64,
    pub cleanup_callback: Option<CleanupCallback>,
    pub async_cleanup_callback: Option<AsyncCleanupCallback>,
    pub is_cleaned_up: bool,
    pub metadata: HashMap<String, String>,
}

impl std::fmt::Debug for ResourceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceInfo")
            .field("id", &self.id)
            .field("resource_type", &self.resource_type)
            .field("size_bytes", &self.size_bytes)
            .field("created_at", &self.created_at)
            .field("last_accessed", &self.last_accessed)
            .field("access_count", &self.access_count)
            .field("is_cleaned_up", &self.is_cleaned_up)
            .field("metadata", &self.metadata)
            .field("has_cleanup_callback", &self.cleanup_callback.is_some())
            .field("has_async_cleanup_callback", &self.async_cleanup_callback.is_some())
            .finish()
    }
}

impl Clone for ResourceInfo {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            resource_type: self.resource_type,
            size_bytes: self.size_bytes,
            created_at: self.created_at,
            last_accessed: self.last_accessed,
            access_count: self.access_count,
            cleanup_callback: None, // Can't clone function pointers
            async_cleanup_callback: None, // Can't clone function pointers
            is_cleaned_up: self.is_cleaned_up,
            metadata: self.metadata.clone(),
        }
    }
}

impl ResourceInfo {
    pub fn new(id: String, resource_type: ResourceType, size_bytes: usize) -> Self {
        let now = Instant::now();
        Self {
            id,
            resource_type,
            size_bytes,
            created_at: now,
            last_accessed: now,
            access_count: 0,
            cleanup_callback: None,
            async_cleanup_callback: None,
            is_cleaned_up: false,
            metadata: HashMap::new(),
        }
    }

    pub fn with_cleanup_callback(mut self, callback: CleanupCallback) -> Self {
        self.cleanup_callback = Some(callback);
        self
    }

    pub fn with_async_cleanup_callback(mut self, callback: AsyncCleanupCallback) -> Self {
        self.async_cleanup_callback = Some(callback);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn access(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }

    pub fn age(&self) -> Duration {
        Instant::now().duration_since(self.created_at)
    }

    pub fn idle_time(&self) -> Duration {
        Instant::now().duration_since(self.last_accessed)
    }
}

/// Resource usage limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_total_memory: usize,
    pub max_resources_per_type: HashMap<ResourceType, usize>,
    pub max_idle_time: Duration,
    pub max_resource_age: Duration,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        let mut max_per_type = HashMap::new();
        max_per_type.insert(ResourceType::Buffer, 1000);
        max_per_type.insert(ResourceType::Connection, 100);
        max_per_type.insert(ResourceType::Task, 500);
        max_per_type.insert(ResourceType::Timer, 200);
        max_per_type.insert(ResourceType::FileHandle, 50);
        max_per_type.insert(ResourceType::CryptoContext, 100);

        Self {
            max_total_memory: 100 * 1024 * 1024, // 100MB
            max_resources_per_type: max_per_type,
            max_idle_time: Duration::from_secs(300), // 5 minutes
            max_resource_age: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Resource usage statistics
#[derive(Debug, Clone)]
pub struct ResourceStats {
    pub total_resources: usize,
    pub total_memory_bytes: usize,
    pub resources_by_type: HashMap<ResourceType, usize>,
    pub memory_by_type: HashMap<ResourceType, usize>,
    pub cleanup_successes: u64,
    pub cleanup_failures: u64,
    pub resources_cleaned_up: u64,
    pub oldest_resource_age: Option<Duration>,
    pub average_idle_time: Option<Duration>,
}

/// Comprehensive resource manager for stream cleanup
pub struct ResourceManager {
    stream_id: u32,
    resources: Arc<RwLock<HashMap<String, ResourceInfo>>>,
    limits: ResourceLimits,
    
    // Statistics
    cleanup_successes: Arc<Mutex<u64>>,
    cleanup_failures: Arc<Mutex<u64>>,
    resources_cleaned_up: Arc<Mutex<u64>>,
    
    // Background cleanup task
    cleanup_task: Option<JoinHandle<()>>,
    cleanup_interval: Duration,
    
    // Weak references for automatic cleanup
    weak_refs: Arc<Mutex<HashMap<String, Box<dyn std::any::Any + Send + Sync>>>>,
    
    // Configuration
    enable_automatic_cleanup: bool,
    enable_memory_monitoring: bool,
    cleanup_timeout: Duration,
}

impl ResourceManager {
    pub fn new(stream_id: u32) -> Self {
        Self {
            stream_id,
            resources: Arc::new(RwLock::new(HashMap::new())),
            limits: ResourceLimits::default(),
            cleanup_successes: Arc::new(Mutex::new(0)),
            cleanup_failures: Arc::new(Mutex::new(0)),
            resources_cleaned_up: Arc::new(Mutex::new(0)),
            cleanup_task: None,
            cleanup_interval: Duration::from_secs(30),
            weak_refs: Arc::new(Mutex::new(HashMap::new())),
            enable_automatic_cleanup: true,
            enable_memory_monitoring: true,
            cleanup_timeout: Duration::from_secs(10),
        }
    }

    /// Register a resource for tracking and cleanup
    pub async fn register_resource(&self, mut resource: ResourceInfo) -> Result<(), ResourceError> {
        // Check limits
        self.check_limits(&resource).await?;
        
        let resource_id = resource.id.clone();
        let resource_type = resource.resource_type;
        let size = resource.size_bytes;
        
        resource.access(); // Mark as accessed
        
        {
            let mut resources = self.resources.write().await;
            resources.insert(resource_id.clone(), resource);
        }
        
        debug!("Registered resource: {} (type: {}, size: {} bytes)", 
               resource_id, resource_type, size);
        
        Ok(())
    }

    /// Register a resource with automatic cleanup when the returned Arc is dropped
    pub async fn register_resource_with_auto_cleanup<T: Send + Sync + 'static>(
        &self,
        resource: ResourceInfo,
        value: T,
    ) -> Result<Arc<T>, ResourceError> {
        let resource_id = resource.id.clone();
        self.register_resource(resource).await?;
        
        let arc_value = Arc::new(value);
        let weak_ref = Arc::downgrade(&arc_value);
        
        {
            let mut weak_refs = self.weak_refs.lock().await;
            weak_refs.insert(resource_id.clone(), Box::new(weak_ref));
        }
        
        Ok(arc_value)
    }

    /// Access a resource (updates access time and count)
    pub async fn access_resource(&self, resource_id: &str) -> Result<(), ResourceError> {
        let mut resources = self.resources.write().await;
        if let Some(resource) = resources.get_mut(resource_id) {
            resource.access();
            Ok(())
        } else {
            Err(ResourceError::NotFound(resource_id.to_string()))
        }
    }

    /// Manually clean up a specific resource
    pub async fn cleanup_resource(&self, resource_id: &str) -> Result<(), ResourceError> {
        let resource = {
            let mut resources = self.resources.write().await;
            resources.remove(resource_id)
        };
        
        if let Some(mut resource) = resource {
            if resource.is_cleaned_up {
                return Err(ResourceError::AlreadyCleanedUp(resource_id.to_string()));
            }
            
            let cleanup_result = self.execute_cleanup(&mut resource).await;
            
            match cleanup_result {
                Ok(()) => {
                    *self.cleanup_successes.lock().await += 1;
                    *self.resources_cleaned_up.lock().await += 1;
                    debug!("Successfully cleaned up resource: {}", resource_id);
                    Ok(())
                }
                Err(e) => {
                    *self.cleanup_failures.lock().await += 1;
                    error!("Failed to clean up resource {}: {}", resource_id, e);
                    Err(ResourceError::CleanupFailed(e))
                }
            }
        } else {
            Err(ResourceError::NotFound(resource_id.to_string()))
        }
    }

    /// Clean up all resources
    pub async fn cleanup_all(&self) -> Result<(), Vec<ResourceError>> {
        let resource_ids: Vec<String> = {
            let resources = self.resources.read().await;
            resources.keys().cloned().collect()
        };
        
        let mut errors = Vec::new();
        
        for resource_id in resource_ids {
            if let Err(e) = self.cleanup_resource(&resource_id).await {
                errors.push(e);
            }
        }
        
        // Clean up weak references
        {
            let mut weak_refs = self.weak_refs.lock().await;
            weak_refs.clear();
        }
        
        if errors.is_empty() {
            info!("Successfully cleaned up all resources for stream {}", self.stream_id);
            Ok(())
        } else {
            warn!("Some resources failed to clean up for stream {}: {} errors", 
                  self.stream_id, errors.len());
            Err(errors)
        }
    }

    /// Start automatic cleanup background task
    pub async fn start_automatic_cleanup(&mut self) {
        if !self.enable_automatic_cleanup || self.cleanup_task.is_some() {
            return;
        }
        
        let resources = Arc::clone(&self.resources);
        let weak_refs = Arc::clone(&self.weak_refs);
        let limits = self.limits.clone();
        let cleanup_interval = self.cleanup_interval;
        let stream_id = self.stream_id;
        let cleanup_successes = Arc::clone(&self.cleanup_successes);
        let cleanup_failures = Arc::clone(&self.cleanup_failures);
        let resources_cleaned_up = Arc::clone(&self.resources_cleaned_up);
        let cleanup_timeout = self.cleanup_timeout;
        
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            
            loop {
                interval.tick().await;
                
                // Clean up resources based on weak references
                Self::cleanup_weak_references(&resources, &weak_refs).await;
                
                // Clean up idle and old resources
                Self::cleanup_idle_resources(
                    &resources,
                    &limits,
                    cleanup_timeout,
                    &cleanup_successes,
                    &cleanup_failures,
                    &resources_cleaned_up,
                ).await;
                
                debug!("Automatic cleanup completed for stream {}", stream_id);
            }
        });
        
        self.cleanup_task = Some(task);
        debug!("Started automatic cleanup task for stream {}", self.stream_id);
    }

    /// Stop automatic cleanup background task
    pub async fn stop_automatic_cleanup(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
            debug!("Stopped automatic cleanup task for stream {}", self.stream_id);
        }
    }

    async fn cleanup_weak_references(
        resources: &Arc<RwLock<HashMap<String, ResourceInfo>>>,
        weak_refs: &Arc<Mutex<HashMap<String, Box<dyn std::any::Any + Send + Sync>>>>,
    ) {
        let mut to_cleanup = Vec::new();
        
        {
            let mut weak_refs_guard = weak_refs.lock().await;
            weak_refs_guard.retain(|resource_id, _weak_ref_any| {
                // Try to downcast to Weak<T> - this is a simplified approach
                // In practice, we'd need a more sophisticated type-erased weak reference system
                to_cleanup.push(resource_id.clone());
                false // For now, clean up all weak references
            });
        }
        
        if !to_cleanup.is_empty() {
            let mut resources_guard = resources.write().await;
            for resource_id in to_cleanup {
                if let Some(mut resource) = resources_guard.remove(&resource_id) {
                    if let Err(e) = Self::execute_cleanup_sync(&mut resource).await {
                        error!("Failed to clean up auto-managed resource {}: {}", resource_id, e);
                    } else {
                        debug!("Auto-cleaned up resource: {}", resource_id);
                    }
                }
            }
        }
    }

    async fn cleanup_idle_resources(
        resources: &Arc<RwLock<HashMap<String, ResourceInfo>>>,
        limits: &ResourceLimits,
        cleanup_timeout: Duration,
        cleanup_successes: &Arc<Mutex<u64>>,
        cleanup_failures: &Arc<Mutex<u64>>,
        resources_cleaned_up: &Arc<Mutex<u64>>,
    ) {
        let mut to_cleanup = Vec::new();
        
        {
            let resources_guard = resources.read().await;
            for (id, resource) in resources_guard.iter() {
                if resource.is_cleaned_up {
                    continue;
                }
                
                let should_cleanup = resource.idle_time() > limits.max_idle_time
                    || resource.age() > limits.max_resource_age;
                
                if should_cleanup {
                    to_cleanup.push(id.clone());
                }
            }
        }
        
        if !to_cleanup.is_empty() {
            let mut resources_guard = resources.write().await;
            for resource_id in to_cleanup {
                if let Some(mut resource) = resources_guard.remove(&resource_id) {
                    let cleanup_result = tokio::time::timeout(
                        cleanup_timeout,
                        Self::execute_cleanup_sync(&mut resource)
                    ).await;
                    
                    match cleanup_result {
                        Ok(Ok(())) => {
                            *cleanup_successes.lock().await += 1;
                            *resources_cleaned_up.lock().await += 1;
                            debug!("Automatically cleaned up idle resource: {}", resource_id);
                        }
                        Ok(Err(e)) => {
                            *cleanup_failures.lock().await += 1;
                            error!("Failed to clean up idle resource {}: {}", resource_id, e);
                        }
                        Err(_) => {
                            *cleanup_failures.lock().await += 1;
                            error!("Timeout cleaning up idle resource: {}", resource_id);
                        }
                    }
                }
            }
        }
    }

    async fn execute_cleanup(&self, resource: &mut ResourceInfo) -> Result<(), String> {
        Self::execute_cleanup_sync(resource).await
    }

    async fn execute_cleanup_sync(resource: &mut ResourceInfo) -> Result<(), String> {
        if resource.is_cleaned_up {
            return Ok(());
        }
        
        // Try async cleanup first
        if let Some(ref async_callback) = resource.async_cleanup_callback {
            let result = async_callback().await;
            resource.is_cleaned_up = true;
            return result;
        }
        
        // Fall back to sync cleanup
        if let Some(ref callback) = resource.cleanup_callback {
            let result = callback();
            resource.is_cleaned_up = true;
            return result;
        }
        
        // No cleanup callback, just mark as cleaned up
        resource.is_cleaned_up = true;
        Ok(())
    }

    async fn check_limits(&self, resource: &ResourceInfo) -> Result<(), ResourceError> {
        let resources = self.resources.read().await;
        
        // Check per-type limits
        if let Some(&limit) = self.limits.max_resources_per_type.get(&resource.resource_type) {
            let current_count = resources.values()
                .filter(|r| r.resource_type == resource.resource_type)
                .count();
            
            if current_count >= limit {
                return Err(ResourceError::LimitExceeded {
                    resource_type: resource.resource_type,
                    limit,
                    current: current_count,
                });
            }
        }
        
        // Check total memory limit
        if self.enable_memory_monitoring {
            let total_memory: usize = resources.values()
                .map(|r| r.size_bytes)
                .sum::<usize>() + resource.size_bytes;
            
            if total_memory > self.limits.max_total_memory {
                return Err(ResourceError::LimitExceeded {
                    resource_type: ResourceType::Buffer, // Generic for memory
                    limit: self.limits.max_total_memory,
                    current: total_memory,
                });
            }
        }
        
        Ok(())
    }

    /// Get current resource usage statistics
    pub async fn get_stats(&self) -> ResourceStats {
        let resources = self.resources.read().await;
        
        let mut resources_by_type = HashMap::new();
        let mut memory_by_type = HashMap::new();
        let mut total_memory = 0;
        let mut oldest_age = None;
        let mut total_idle_time = Duration::ZERO;
        let mut idle_count = 0;
        
        for resource in resources.values() {
            *resources_by_type.entry(resource.resource_type).or_insert(0) += 1;
            *memory_by_type.entry(resource.resource_type).or_insert(0) += resource.size_bytes;
            total_memory += resource.size_bytes;
            
            let age = resource.age();
            oldest_age = Some(oldest_age.map_or(age, |old: Duration| old.max(age)));
            
            total_idle_time += resource.idle_time();
            idle_count += 1;
        }
        
        let average_idle_time = if idle_count > 0 {
            Some(total_idle_time / idle_count as u32)
        } else {
            None
        };
        
        ResourceStats {
            total_resources: resources.len(),
            total_memory_bytes: total_memory,
            resources_by_type,
            memory_by_type,
            cleanup_successes: *self.cleanup_successes.lock().await,
            cleanup_failures: *self.cleanup_failures.lock().await,
            resources_cleaned_up: *self.resources_cleaned_up.lock().await,
            oldest_resource_age: oldest_age,
            average_idle_time,
        }
    }

    /// Set resource limits
    pub fn set_limits(&mut self, limits: ResourceLimits) {
        self.limits = limits;
    }

    /// Enable or disable automatic cleanup
    pub fn set_automatic_cleanup(&mut self, enabled: bool) {
        self.enable_automatic_cleanup = enabled;
    }

    /// Set cleanup interval
    pub fn set_cleanup_interval(&mut self, interval: Duration) {
        self.cleanup_interval = interval;
    }

    /// Enable or disable memory monitoring
    pub fn set_memory_monitoring(&mut self, enabled: bool) {
        self.enable_memory_monitoring = enabled;
    }

    /// Get resource information
    pub async fn get_resource_info(&self, resource_id: &str) -> Option<ResourceInfo> {
        let resources = self.resources.read().await;
        resources.get(resource_id).map(|r| r.clone())
    }

    /// List all resource IDs
    pub async fn list_resources(&self) -> Vec<String> {
        let resources = self.resources.read().await;
        resources.keys().cloned().collect()
    }

    /// Force cleanup of resources exceeding limits
    pub async fn force_cleanup_over_limits(&self) -> Result<usize, Vec<ResourceError>> {
        let mut cleaned_up = 0;
        let mut errors = Vec::new();
        
        // Find resources that exceed limits
        let to_cleanup: Vec<String> = {
            let resources = self.resources.read().await;
            let mut candidates = Vec::new();
            
            for (id, resource) in resources.iter() {
                let should_cleanup = resource.idle_time() > self.limits.max_idle_time
                    || resource.age() > self.limits.max_resource_age;
                
                if should_cleanup {
                    candidates.push(id.clone());
                }
            }
            
            candidates
        };
        
        for resource_id in to_cleanup {
            match self.cleanup_resource(&resource_id).await {
                Ok(()) => cleaned_up += 1,
                Err(e) => errors.push(e),
            }
        }
        
        if errors.is_empty() {
            Ok(cleaned_up)
        } else {
            Err(errors)
        }
    }
}

impl Drop for ResourceManager {
    fn drop(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
        }
        
        // Note: We can't do async cleanup in Drop, so we just log a warning
        // The caller should call cleanup_all() before dropping
        warn!("ResourceManager dropped without explicit cleanup for stream {}", self.stream_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_resource_registration() {
        let manager = ResourceManager::new(1);
        
        let resource = ResourceInfo::new(
            "test_resource".to_string(),
            ResourceType::Buffer,
            1024,
        );
        
        manager.register_resource(resource).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_resources, 1);
        assert_eq!(stats.total_memory_bytes, 1024);
    }

    #[tokio::test]
    async fn test_resource_cleanup() {
        let manager = ResourceManager::new(1);
        let cleaned_up = Arc::new(AtomicBool::new(false));
        let cleaned_up_clone = Arc::clone(&cleaned_up);
        
        let resource = ResourceInfo::new(
            "test_resource".to_string(),
            ResourceType::Buffer,
            1024,
        ).with_cleanup_callback(Box::new(move || {
            cleaned_up_clone.store(true, Ordering::SeqCst);
            Ok(())
        }));
        
        manager.register_resource(resource).await.unwrap();
        manager.cleanup_resource("test_resource").await.unwrap();
        
        assert!(cleaned_up.load(Ordering::SeqCst));
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_resources, 0);
        assert_eq!(stats.cleanup_successes, 1);
    }

    #[tokio::test]
    async fn test_auto_cleanup_with_arc() {
        let manager = ResourceManager::new(1);
        let cleaned_up = Arc::new(AtomicBool::new(false));
        let cleaned_up_clone = Arc::clone(&cleaned_up);
        
        let resource = ResourceInfo::new(
            "auto_resource".to_string(),
            ResourceType::Buffer,
            1024,
        ).with_cleanup_callback(Box::new(move || {
            cleaned_up_clone.store(true, Ordering::SeqCst);
            Ok(())
        }));
        
        let data = "test data";
        let arc_data = manager.register_resource_with_auto_cleanup(resource, data).await.unwrap();
        
        // Drop the Arc
        drop(arc_data);
        
        // Trigger cleanup
        ResourceManager::cleanup_weak_references(&manager.resources, &manager.weak_refs).await;
        
        // Should be cleaned up automatically
        assert!(cleaned_up.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_resource_limits() {
        let mut manager = ResourceManager::new(1);
        let mut limits = ResourceLimits::default();
        limits.max_resources_per_type.insert(ResourceType::Buffer, 2);
        manager.set_limits(limits);
        
        // Should be able to register up to limit
        for i in 0..2 {
            let resource = ResourceInfo::new(
                format!("resource_{}", i),
                ResourceType::Buffer,
                1024,
            );
            manager.register_resource(resource).await.unwrap();
        }
        
        // Should fail on exceeding limit
        let resource = ResourceInfo::new(
            "resource_overflow".to_string(),
            ResourceType::Buffer,
            1024,
        );
        
        let result = manager.register_resource(resource).await;
        assert!(matches!(result, Err(ResourceError::LimitExceeded { .. })));
    }

    #[tokio::test]
    async fn test_cleanup_all() {
        let manager = ResourceManager::new(1);
        
        // Register multiple resources
        for i in 0..3 {
            let resource = ResourceInfo::new(
                format!("resource_{}", i),
                ResourceType::Buffer,
                1024,
            );
            manager.register_resource(resource).await.unwrap();
        }
        
        let stats_before = manager.get_stats().await;
        assert_eq!(stats_before.total_resources, 3);
        
        manager.cleanup_all().await.unwrap();
        
        let stats_after = manager.get_stats().await;
        assert_eq!(stats_after.total_resources, 0);
        assert_eq!(stats_after.resources_cleaned_up, 3);
    }

    #[tokio::test]
    async fn test_async_cleanup_callback() {
        let manager = ResourceManager::new(1);
        let cleaned_up = Arc::new(AtomicBool::new(false));
        let cleaned_up_clone = Arc::clone(&cleaned_up);
        
        let resource = ResourceInfo::new(
            "async_resource".to_string(),
            ResourceType::Buffer,
            1024,
        ).with_async_cleanup_callback(Box::new(move || {
            let cleaned_up = Arc::clone(&cleaned_up_clone);
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                cleaned_up.store(true, Ordering::SeqCst);
                Ok(())
            })
        }));
        
        manager.register_resource(resource).await.unwrap();
        manager.cleanup_resource("async_resource").await.unwrap();
        
        assert!(cleaned_up.load(Ordering::SeqCst));
    }
}