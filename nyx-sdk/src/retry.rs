#![forbid(unsafe_code)]

//! Retry strategies and error recovery for the Nyx SDK.
//!
//! This module provides comprehensive retry logic with exponential backoff,
//! circuit breaker patterns, and intelligent error classification for
//! robust error handling and recovery.

use crate::error::{NyxError, NyxResult, ErrorSeverity};
use std::time::{Duration, Instant};
use std::future::Future;
use std::pin::Pin;
use tokio::time::sleep;
use tracing::{debug, warn, error};
use serde::{Deserialize, Serialize};

/// Retry strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStrategy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Add jitter to retry delays
    pub jitter: bool,
    /// Maximum total time to spend retrying
    pub max_total_time: Option<Duration>,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
            max_total_time: Some(Duration::from_secs(300)), // 5 minutes
        }
    }
}

/// Retry context tracking retry attempts and timing
#[derive(Debug, Clone)]
pub struct RetryContext {
    /// Current attempt number (1-based)
    pub attempt: u32,
    /// Time when retries started
    pub start_time: Instant,
    /// Last error encountered
    pub last_error: Option<NyxError>,
    /// Total delay time so far
    pub total_delay: Duration,
}

impl RetryContext {
    /// Create a new retry context
    pub fn new() -> Self {
        Self {
            attempt: 0,
            start_time: Instant::now(),
            last_error: None,
            total_delay: Duration::ZERO,
        }
    }
    
    /// Get elapsed time since retries started
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// Check if we should continue retrying
    pub fn should_continue(&self, strategy: &RetryStrategy) -> bool {
        // Check attempt limit
        if self.attempt >= strategy.max_attempts {
            return false;
        }
        
        // Check total time limit
        if let Some(max_time) = strategy.max_total_time {
            if self.elapsed() >= max_time {
                return false;
            }
        }
        
        // Check if last error is retryable
        if let Some(ref error) = self.last_error {
            if !error.is_retryable() {
                return false;
            }
        }
        
        true
    }
    
    /// Calculate next retry delay
    pub fn next_delay(&self, strategy: &RetryStrategy) -> Duration {
        let base_delay = if self.attempt == 0 {
            strategy.initial_delay
        } else {
            let multiplier = strategy.backoff_multiplier.powi((self.attempt - 1) as i32);
            let delay = strategy.initial_delay.mul_f64(multiplier);
            std::cmp::min(delay, strategy.max_delay)
        };
        
        if strategy.jitter {
            // Add ±25% jitter
            let jitter_factor = 0.75 + (fastrand::f64() * 0.5); // 0.75 to 1.25
            base_delay.mul_f64(jitter_factor)
        } else {
            base_delay
        }
    }
}

impl Default for RetryContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Retry executor that handles the retry logic
pub struct RetryExecutor {
    strategy: RetryStrategy,
}

impl RetryExecutor {
    /// Create a new retry executor with the given strategy
    pub fn new(strategy: RetryStrategy) -> Self {
        Self { strategy }
    }
    
    /// Create a retry executor with default strategy
    pub fn default() -> Self {
        Self::new(RetryStrategy::default())
    }
    
    /// Execute an operation with retry logic
    pub async fn execute<F, Fut, T>(&self, operation: F) -> NyxResult<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = NyxResult<T>>,
    {
        let mut context = RetryContext::new();
        
        loop {
            context.attempt += 1;
            
            debug!(
                "Executing operation (attempt {}/{})",
                context.attempt, self.strategy.max_attempts
            );
            
            match operation().await {
                Ok(result) => {
                    if context.attempt > 1 {
                        debug!(
                            "Operation succeeded after {} attempts in {:?}",
                            context.attempt,
                            context.elapsed()
                        );
                    }
                    return Ok(result);
                }
                Err(error) => {
                    context.last_error = Some(error.clone());
                    
                    if !context.should_continue(&self.strategy) {
                        error!(
                            "Operation failed after {} attempts in {:?}: {}",
                            context.attempt,
                            context.elapsed(),
                            error
                        );
                        
                        // Add retry context to error if possible
                        return Err(self.enhance_error_with_context(error, &context));
                    }
                    
                    let delay = context.next_delay(&self.strategy);
                    context.total_delay += delay;
                    
                    warn!(
                        "Operation failed (attempt {}/{}), retrying in {:?}: {}",
                        context.attempt, self.strategy.max_attempts, delay, error
                    );
                    
                    // Add troubleshooting info for high severity errors
                    if error.severity() >= ErrorSeverity::High {
                        if let Some(info) = error.troubleshooting_info() {
                            warn!("Troubleshooting: {}", info);
                        }
                    }
                    
                    sleep(delay).await;
                }
            }
        }
    }
    
    /// Execute an operation with custom retry condition
    pub async fn execute_with_condition<F, Fut, T, C>(
        &self,
        operation: F,
        should_retry: C,
    ) -> NyxResult<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = NyxResult<T>>,
        C: Fn(&NyxError, &RetryContext) -> bool,
    {
        let mut context = RetryContext::new();
        
        loop {
            context.attempt += 1;
            
            match operation().await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    context.last_error = Some(error.clone());
                    
                    if !should_retry(&error, &context) || !context.should_continue(&self.strategy) {
                        return Err(self.enhance_error_with_context(error, &context));
                    }
                    
                    let delay = context.next_delay(&self.strategy);
                    context.total_delay += delay;
                    
                    sleep(delay).await;
                }
            }
        }
    }
    
    /// Enhance error with retry context information
    fn enhance_error_with_context(&self, mut error: NyxError, context: &RetryContext) -> NyxError {
        // For now, we'll just return the original error
        // In the future, we could add retry context to the error message
        match error {
            NyxError::DaemonConnection { ref mut message, .. } => {
                *message = format!(
                    "{} (failed after {} attempts in {:?})",
                    message,
                    context.attempt,
                    context.elapsed()
                );
            }
            NyxError::Network { ref mut message, .. } => {
                *message = format!(
                    "{} (failed after {} attempts in {:?})",
                    message,
                    context.attempt,
                    context.elapsed()
                );
            }
            _ => {}
        }
        
        error
    }
}

/// Convenience function to retry an operation with default strategy
pub async fn retry<F, Fut, T>(operation: F) -> NyxResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = NyxResult<T>>,
{
    RetryExecutor::default().execute(operation).await
}

/// Convenience function to retry an operation with custom strategy
pub async fn retry_with_strategy<F, Fut, T>(
    strategy: RetryStrategy,
    operation: F,
) -> NyxResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = NyxResult<T>>,
{
    RetryExecutor::new(strategy).execute(operation).await
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current state of the circuit breaker
    state: CircuitBreakerState,
    /// Configuration
    config: CircuitBreakerConfig,
    /// Failure count in current window
    failure_count: u32,
    /// Success count in half-open state
    success_count: u32,
    /// Last state change time
    last_state_change: Instant,
}

/// Circuit breaker state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Circuit is closed, allowing requests
    Closed,
    /// Circuit is open, blocking requests
    Open,
    /// Circuit is half-open, testing recovery
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Duration to keep circuit open
    pub open_timeout: Duration,
    /// Number of successes needed to close circuit from half-open
    pub success_threshold: u32,
    /// Time window for counting failures
    pub failure_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_timeout: Duration::from_secs(60),
            success_threshold: 3,
            failure_window: Duration::from_secs(60),
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            config,
            failure_count: 0,
            success_count: 0,
            last_state_change: Instant::now(),
        }
    }
    
    /// Create a circuit breaker with default configuration
    pub fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
    
    /// Check if a request can be made
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if self.last_state_change.elapsed() >= self.config.open_timeout {
                    self.transition_to_half_open();
                    true
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }
    
    /// Record a successful operation
    pub fn record_success(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.transition_to_closed();
                }
            }
            CircuitBreakerState::Open => {
                // Should not happen
            }
        }
    }
    
    /// Record a failed operation
    pub fn record_failure(&mut self) {
        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.transition_to_open();
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.transition_to_open();
            }
            CircuitBreakerState::Open => {
                // Already open
            }
        }
    }
    
    /// Get current state
    pub fn state(&self) -> &CircuitBreakerState {
        &self.state
    }
    
    /// Execute an operation with circuit breaker protection
    pub async fn execute<F, Fut, T>(&mut self, operation: F) -> NyxResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = NyxResult<T>>,
    {
        if !self.can_execute() {
            return Err(NyxError::network_error(
                "Circuit breaker is open - operation blocked",
                None,
            ));
        }
        
        match operation().await {
            Ok(result) => {
                self.record_success();
                Ok(result)
            }
            Err(error) => {
                self.record_failure();
                Err(error)
            }
        }
    }
    
    /// Transition to closed state
    fn transition_to_closed(&mut self) {
        debug!("Circuit breaker transitioning to CLOSED");
        self.state = CircuitBreakerState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.last_state_change = Instant::now();
    }
    
    /// Transition to open state
    fn transition_to_open(&mut self) {
        warn!("Circuit breaker transitioning to OPEN");
        self.state = CircuitBreakerState::Open;
        self.success_count = 0;
        self.last_state_change = Instant::now();
    }
    
    /// Transition to half-open state
    fn transition_to_half_open(&mut self) {
        debug!("Circuit breaker transitioning to HALF-OPEN");
        self.state = CircuitBreakerState::HalfOpen;
        self.success_count = 0;
        self.last_state_change = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_retry_success() {
        let mut attempts = 0;
        let result = retry(|| async {
            attempts += 1;
            if attempts < 3 {
                Err(NyxError::network_error("temporary failure", None))
            } else {
                Ok("success")
            }
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts, 3);
    }
    
    #[tokio::test]
    async fn test_retry_failure() {
        let mut attempts = 0;
        let result = retry(|| async {
            attempts += 1;
            Err(NyxError::network_error("persistent failure", None))
        }).await;
        
        assert!(result.is_err());
        assert_eq!(attempts, 3); // Default max attempts
    }
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let mut breaker = CircuitBreaker::default();
        
        // Initially closed
        assert_eq!(breaker.state(), &CircuitBreakerState::Closed);
        assert!(breaker.can_execute());
        
        // Record failures to open circuit
        for _ in 0..5 {
            breaker.record_failure();
        }
        assert_eq!(breaker.state(), &CircuitBreakerState::Open);
        assert!(!breaker.can_execute());
    }
}