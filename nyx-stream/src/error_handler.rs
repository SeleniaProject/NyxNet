#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::fmt;
use bytes::Bytes;
use tracing::{debug, warn, error};

/// Stream error categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Network-related errors (timeouts, connection issues)
    Network,
    /// Protocol violations (invalid frames, sequence errors)
    Protocol,
    /// Flow control violations
    FlowControl,
    /// Resource exhaustion (buffer overflow, memory)
    Resource,
    /// Cryptographic errors
    Crypto,
    /// Application-level errors
    Application,
    /// Internal system errors
    Internal,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::Network => write!(f, "Network"),
            ErrorCategory::Protocol => write!(f, "Protocol"),
            ErrorCategory::FlowControl => write!(f, "FlowControl"),
            ErrorCategory::Resource => write!(f, "Resource"),
            ErrorCategory::Crypto => write!(f, "Crypto"),
            ErrorCategory::Application => write!(f, "Application"),
            ErrorCategory::Internal => write!(f, "Internal"),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Informational - no action needed
    Info,
    /// Warning - degraded performance but recoverable
    Warning,
    /// Error - significant issue but stream can continue
    Error,
    /// Critical - stream must be terminated
    Critical,
    /// Fatal - entire connection must be terminated
    Fatal,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "INFO"),
            ErrorSeverity::Warning => write!(f, "WARN"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
            ErrorSeverity::Fatal => write!(f, "FATAL"),
        }
    }
}

/// Recovery strategy for different error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// No recovery needed
    None,
    /// Retry the operation with exponential backoff
    Retry { max_attempts: u32, base_delay: Duration },
    /// Reset the stream state
    Reset,
    /// Gracefully close the stream
    Close,
    /// Terminate the connection
    Terminate,
    /// Custom recovery with specific actions
    Custom(String),
}

/// Detailed error context information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub stream_id: Option<u32>,
    pub timestamp: Instant,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub message: String,
    pub cause: Option<String>,
    pub recovery_strategy: RecoveryStrategy,
    pub metadata: HashMap<String, String>,
    pub partial_data: Option<Bytes>,
}

impl ErrorContext {
    pub fn new(
        category: ErrorCategory,
        severity: ErrorSeverity,
        message: String,
    ) -> Self {
        Self {
            stream_id: None,
            timestamp: Instant::now(),
            category,
            severity,
            message,
            cause: None,
            recovery_strategy: RecoveryStrategy::None,
            metadata: HashMap::new(),
            partial_data: None,
        }
    }

    pub fn with_stream_id(mut self, stream_id: u32) -> Self {
        self.stream_id = Some(stream_id);
        self
    }

    pub fn with_cause(mut self, cause: String) -> Self {
        self.cause = Some(cause);
        self
    }

    pub fn with_recovery(mut self, strategy: RecoveryStrategy) -> Self {
        self.recovery_strategy = strategy;
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_partial_data(mut self, data: Bytes) -> Self {
        self.partial_data = Some(data);
        self
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(
            self.recovery_strategy,
            RecoveryStrategy::Retry { .. } | RecoveryStrategy::Reset | RecoveryStrategy::Custom(_)
        )
    }

    pub fn should_close_stream(&self) -> bool {
        matches!(
            self.recovery_strategy,
            RecoveryStrategy::Close | RecoveryStrategy::Terminate
        ) || self.severity >= ErrorSeverity::Critical
    }

    pub fn should_terminate_connection(&self) -> bool {
        matches!(self.recovery_strategy, RecoveryStrategy::Terminate) 
            || self.severity == ErrorSeverity::Fatal
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} {}: {}",
            self.severity,
            self.category,
            self.stream_id.map_or("GLOBAL".to_string(), |id| format!("STREAM-{}", id)),
            self.message
        )?;
        
        if let Some(ref cause) = self.cause {
            write!(f, " (caused by: {})", cause)?;
        }
        
        Ok(())
    }
}

/// Retry state for error recovery
#[derive(Debug)]
struct RetryState {
    attempts: u32,
    max_attempts: u32,
    base_delay: Duration,
    next_retry: Instant,
    last_error: Option<ErrorContext>,
}

impl RetryState {
    fn new(max_attempts: u32, base_delay: Duration) -> Self {
        Self {
            attempts: 0,
            max_attempts,
            base_delay,
            next_retry: Instant::now(),
            last_error: None,
        }
    }

    fn can_retry(&self) -> bool {
        self.attempts < self.max_attempts && Instant::now() >= self.next_retry
    }

    #[cfg(test)]
    fn can_retry_ignore_delay(&self) -> bool {
        self.attempts < self.max_attempts
    }

    fn record_attempt(&mut self, error: ErrorContext) {
        self.attempts += 1;
        self.last_error = Some(error);
        
        // Exponential backoff with jitter
        let delay_multiplier = 2_u32.pow(self.attempts.min(10));
        let delay = self.base_delay * delay_multiplier;
        let jitter = Duration::from_millis(fastrand::u64(0..=delay.as_millis() as u64 / 4));
        
        self.next_retry = Instant::now() + delay + jitter;
        
        debug!("Retry attempt {} of {}, next retry in {:?}", 
               self.attempts, self.max_attempts, delay + jitter);
    }

    fn reset(&mut self) {
        self.attempts = 0;
        self.next_retry = Instant::now();
        self.last_error = None;
    }
}

/// Stream error handler with recovery mechanisms
pub struct StreamErrorHandler {
    stream_id: u32,
    error_history: Vec<ErrorContext>,
    retry_states: HashMap<ErrorCategory, RetryState>,
    max_history_size: usize,
    
    // Error classification rules
    classification_rules: HashMap<String, (ErrorCategory, ErrorSeverity, RecoveryStrategy)>,
    
    // Statistics
    error_counts: HashMap<ErrorCategory, u32>,
    recovery_successes: u32,
    recovery_failures: u32,
    
    // Configuration
    enable_auto_recovery: bool,
    max_consecutive_errors: u32,
    consecutive_errors: u32,
}

impl StreamErrorHandler {
    pub fn new(stream_id: u32) -> Self {
        let mut handler = Self {
            stream_id,
            error_history: Vec::new(),
            retry_states: HashMap::new(),
            max_history_size: 100,
            classification_rules: HashMap::new(),
            error_counts: HashMap::new(),
            recovery_successes: 0,
            recovery_failures: 0,
            enable_auto_recovery: true,
            max_consecutive_errors: 10,
            consecutive_errors: 0,
        };
        
        handler.setup_default_rules();
        handler
    }

    fn setup_default_rules(&mut self) {
        // Network errors
        self.add_classification_rule(
            "connection_timeout".to_string(),
            ErrorCategory::Network,
            ErrorSeverity::Error,
            RecoveryStrategy::Retry { max_attempts: 3, base_delay: Duration::from_millis(100) },
        );
        
        self.add_classification_rule(
            "connection_reset".to_string(),
            ErrorCategory::Network,
            ErrorSeverity::Critical,
            RecoveryStrategy::Reset,
        );
        
        // Protocol errors
        self.add_classification_rule(
            "invalid_frame".to_string(),
            ErrorCategory::Protocol,
            ErrorSeverity::Error,
            RecoveryStrategy::None,
        );
        
        self.add_classification_rule(
            "sequence_error".to_string(),
            ErrorCategory::Protocol,
            ErrorSeverity::Warning,
            RecoveryStrategy::Retry { max_attempts: 1, base_delay: Duration::from_millis(10) },
        );
        
        // Flow control errors
        self.add_classification_rule(
            "flow_control_violation".to_string(),
            ErrorCategory::FlowControl,
            ErrorSeverity::Warning,
            RecoveryStrategy::Retry { max_attempts: 2, base_delay: Duration::from_millis(50) },
        );
        
        // Resource errors
        self.add_classification_rule(
            "buffer_overflow".to_string(),
            ErrorCategory::Resource,
            ErrorSeverity::Error,
            RecoveryStrategy::Reset,
        );
        
        self.add_classification_rule(
            "memory_exhausted".to_string(),
            ErrorCategory::Resource,
            ErrorSeverity::Critical,
            RecoveryStrategy::Close,
        );
        
        // Crypto errors
        self.add_classification_rule(
            "decryption_failed".to_string(),
            ErrorCategory::Crypto,
            ErrorSeverity::Critical,
            RecoveryStrategy::Terminate,
        );
    }

    pub fn add_classification_rule(
        &mut self,
        error_pattern: String,
        category: ErrorCategory,
        severity: ErrorSeverity,
        recovery: RecoveryStrategy,
    ) {
        self.classification_rules.insert(error_pattern, (category, severity, recovery));
    }

    /// Handle an error with automatic classification and recovery
    pub fn handle_error(&mut self, error_message: &str, cause: Option<String>) -> Result<RecoveryAction, ErrorContext> {
        let (category, severity, recovery_strategy) = self.classify_error(error_message);
        
        let mut context = ErrorContext::new(category, severity, error_message.to_string())
            .with_stream_id(self.stream_id)
            .with_recovery(recovery_strategy.clone());
        
        if let Some(cause) = cause {
            context = context.with_cause(cause);
        }
        
        self.record_error(context.clone());
        
        if self.enable_auto_recovery {
            self.attempt_recovery(context)
        } else {
            Err(context)
        }
    }

    /// Handle an error with explicit context
    pub fn handle_error_with_context(&mut self, mut context: ErrorContext) -> Result<RecoveryAction, ErrorContext> {
        if context.stream_id.is_none() {
            context.stream_id = Some(self.stream_id);
        }
        
        self.record_error(context.clone());
        
        if self.enable_auto_recovery {
            self.attempt_recovery(context)
        } else {
            Err(context)
        }
    }

    fn classify_error(&self, error_message: &str) -> (ErrorCategory, ErrorSeverity, RecoveryStrategy) {
        // Try to match against classification rules
        for (pattern, (category, severity, recovery)) in &self.classification_rules {
            if error_message.contains(pattern) {
                return (*category, *severity, recovery.clone());
            }
        }
        
        // Default classification
        (ErrorCategory::Internal, ErrorSeverity::Error, RecoveryStrategy::None)
    }

    fn record_error(&mut self, context: ErrorContext) {
        let category = context.category;
        
        // Update statistics
        *self.error_counts.entry(category).or_insert(0) += 1;
        self.consecutive_errors += 1;
        
        // Add to history
        self.error_history.push(context.clone());
        if self.error_history.len() > self.max_history_size {
            self.error_history.remove(0);
        }
        
        // Log the error
        match context.severity {
            ErrorSeverity::Info => debug!("{}", context),
            ErrorSeverity::Warning => warn!("{}", context),
            ErrorSeverity::Error => error!("{}", context),
            ErrorSeverity::Critical => error!("CRITICAL: {}", context),
            ErrorSeverity::Fatal => error!("FATAL: {}", context),
        }
    }

    fn attempt_recovery(&mut self, context: ErrorContext) -> Result<RecoveryAction, ErrorContext> {
        // Check if we've exceeded consecutive error limit
        if self.consecutive_errors > self.max_consecutive_errors {
            warn!("Exceeded maximum consecutive errors ({}), terminating stream", 
                  self.max_consecutive_errors);
            return Ok(RecoveryAction::TerminateStream);
        }
        
        match &context.recovery_strategy {
            RecoveryStrategy::None => Err(context),
            
            RecoveryStrategy::Retry { max_attempts, base_delay } => {
                let retry_state = self.retry_states
                    .entry(context.category)
                    .or_insert_with(|| RetryState::new(*max_attempts, *base_delay));
                
                #[cfg(test)]
                let can_retry = retry_state.can_retry_ignore_delay();
                #[cfg(not(test))]
                let can_retry = retry_state.can_retry();
                
                if can_retry {
                    retry_state.record_attempt(context.clone());
                    debug!("Attempting recovery for {} error (attempt {}/{})", 
                           context.category, retry_state.attempts, retry_state.max_attempts);
                    Ok(RecoveryAction::Retry)
                } else {
                    warn!("Retry limit exceeded for {} errors", context.category);
                    self.recovery_failures += 1;
                    Err(context)
                }
            }
            
            RecoveryStrategy::Reset => {
                debug!("Resetting stream state for recovery");
                self.reset_retry_states();
                Ok(RecoveryAction::ResetStream)
            }
            
            RecoveryStrategy::Close => {
                debug!("Closing stream for recovery");
                Ok(RecoveryAction::CloseStream)
            }
            
            RecoveryStrategy::Terminate => {
                debug!("Terminating connection for recovery");
                Ok(RecoveryAction::TerminateConnection)
            }
            
            RecoveryStrategy::Custom(action) => {
                debug!("Executing custom recovery: {}", action);
                Ok(RecoveryAction::Custom(action.clone()))
            }
        }
    }

    /// Mark a successful recovery
    pub fn mark_recovery_success(&mut self, category: ErrorCategory) {
        if let Some(retry_state) = self.retry_states.get_mut(&category) {
            retry_state.reset();
        }
        
        self.recovery_successes += 1;
        self.consecutive_errors = 0;
        
        debug!("Recovery successful for {} errors", category);
    }

    /// Get partial data from the most recent error if available
    pub fn get_partial_data(&self) -> Option<Bytes> {
        self.error_history
            .last()
            .and_then(|ctx| ctx.partial_data.clone())
    }

    /// Get error statistics
    pub fn get_stats(&self) -> ErrorHandlerStats {
        ErrorHandlerStats {
            stream_id: self.stream_id,
            total_errors: self.error_history.len() as u32,
            error_counts: self.error_counts.clone(),
            recovery_successes: self.recovery_successes,
            recovery_failures: self.recovery_failures,
            consecutive_errors: self.consecutive_errors,
            recent_errors: self.error_history
                .iter()
                .rev()
                .take(5)
                .cloned()
                .collect(),
        }
    }

    /// Get recent error history
    pub fn get_error_history(&self) -> &[ErrorContext] {
        &self.error_history
    }

    /// Clear error history and reset state
    pub fn reset(&mut self) {
        self.error_history.clear();
        self.retry_states.clear();
        self.error_counts.clear();
        self.consecutive_errors = 0;
        debug!("Error handler reset for stream {}", self.stream_id);
    }

    fn reset_retry_states(&mut self) {
        for retry_state in self.retry_states.values_mut() {
            retry_state.reset();
        }
    }

    /// Enable or disable automatic recovery
    pub fn set_auto_recovery(&mut self, enabled: bool) {
        self.enable_auto_recovery = enabled;
    }

    /// Set maximum consecutive errors before termination
    pub fn set_max_consecutive_errors(&mut self, max: u32) {
        self.max_consecutive_errors = max;
    }
}

/// Actions that can be taken for error recovery
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Retry the failed operation
    Retry,
    /// Reset the stream state
    ResetStream,
    /// Close the stream gracefully
    CloseStream,
    /// Terminate the entire connection
    TerminateConnection,
    /// Terminate just this stream
    TerminateStream,
    /// Custom recovery action
    Custom(String),
}

/// Error handler statistics
#[derive(Debug, Clone)]
pub struct ErrorHandlerStats {
    pub stream_id: u32,
    pub total_errors: u32,
    pub error_counts: HashMap<ErrorCategory, u32>,
    pub recovery_successes: u32,
    pub recovery_failures: u32,
    pub consecutive_errors: u32,
    pub recent_errors: Vec<ErrorContext>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_creation() {
        let context = ErrorContext::new(
            ErrorCategory::Network,
            ErrorSeverity::Error,
            "Connection timeout".to_string(),
        )
        .with_stream_id(42)
        .with_cause("Network unreachable".to_string())
        .with_recovery(RecoveryStrategy::Retry { max_attempts: 3, base_delay: Duration::from_millis(100) });
        
        assert_eq!(context.stream_id, Some(42));
        assert_eq!(context.category, ErrorCategory::Network);
        assert_eq!(context.severity, ErrorSeverity::Error);
        assert!(context.is_recoverable());
        assert!(!context.should_close_stream());
    }

    #[test]
    fn test_error_classification() {
        let mut handler = StreamErrorHandler::new(1);
        
        // Test default classification rules
        let (category, severity, _) = handler.classify_error("connection_timeout occurred");
        assert_eq!(category, ErrorCategory::Network);
        assert_eq!(severity, ErrorSeverity::Error);
        
        let (category, severity, _) = handler.classify_error("buffer_overflow detected");
        assert_eq!(category, ErrorCategory::Resource);
        assert_eq!(severity, ErrorSeverity::Error);
    }

    #[test]
    fn test_retry_recovery() {
        let mut handler = StreamErrorHandler::new(1);
        
        // First error should allow retry
        let result = handler.handle_error("connection_timeout", None);
        assert!(matches!(result, Ok(RecoveryAction::Retry)));
        
        // Should be able to retry multiple times
        let result = handler.handle_error("connection_timeout", None);
        assert!(matches!(result, Ok(RecoveryAction::Retry)));
        
        // After max attempts, should fail
        let result = handler.handle_error("connection_timeout", None);
        assert!(matches!(result, Ok(RecoveryAction::Retry)));
        
        let result = handler.handle_error("connection_timeout", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_success() {
        let mut handler = StreamErrorHandler::new(1);
        
        // Generate some errors
        let _ = handler.handle_error("connection_timeout", None);
        assert_eq!(handler.consecutive_errors, 1);
        
        // Mark recovery success
        handler.mark_recovery_success(ErrorCategory::Network);
        assert_eq!(handler.consecutive_errors, 0);
        assert_eq!(handler.recovery_successes, 1);
    }

    #[test]
    fn test_consecutive_error_limit() {
        let mut handler = StreamErrorHandler::new(1);
        handler.set_max_consecutive_errors(3);
        
        // Generate errors up to limit
        for _ in 0..3 {
            let _ = handler.handle_error("unknown_error", None);
        }
        
        // Next error should trigger termination
        let result = handler.handle_error("unknown_error", None);
        assert!(matches!(result, Ok(RecoveryAction::TerminateStream)));
    }

    #[test]
    fn test_error_statistics() {
        let mut handler = StreamErrorHandler::new(1);
        
        // Generate various errors
        let _ = handler.handle_error("connection_timeout", None);
        let _ = handler.handle_error("buffer_overflow", None);
        let _ = handler.handle_error("invalid_frame", None);
        
        let stats = handler.get_stats();
        assert_eq!(stats.total_errors, 3);
        assert!(stats.error_counts.contains_key(&ErrorCategory::Network));
        assert!(stats.error_counts.contains_key(&ErrorCategory::Resource));
        assert!(stats.error_counts.contains_key(&ErrorCategory::Protocol));
    }

    #[test]
    fn test_partial_data_recovery() {
        let mut handler = StreamErrorHandler::new(1);
        
        let partial_data = Bytes::from("partial data");
        let context = ErrorContext::new(
            ErrorCategory::Network,
            ErrorSeverity::Error,
            "Connection lost".to_string(),
        ).with_partial_data(partial_data.clone());
        
        let _ = handler.handle_error_with_context(context);
        
        let recovered_data = handler.get_partial_data();
        assert_eq!(recovered_data, Some(partial_data));
    }
}