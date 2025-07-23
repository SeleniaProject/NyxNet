//! Adaptive Cover Traffic Generator with Mobile Power State Integration
//!
//! This module implements an advanced cover traffic system that adapts to:
//! - Mobile device power states (battery level, charging status, screen state)
//! - Network conditions and bandwidth availability
//! - User activity patterns and application state
//! - Time-of-day and usage patterns
//!
//! The system uses Poisson distribution for traffic generation with dynamic
//! lambda (λ) parameter adjustment based on real-time conditions.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::VecDeque;
use tokio::sync::{RwLock, broadcast, watch};
use tokio::time::{interval, sleep_until, Instant as TokioInstant};
use serde::{Serialize, Deserialize};
use tracing::{debug, info, trace};
use rand::{Rng, SeedableRng};
use rand_distr::{Poisson, Distribution};

// Import mobile types
use nyx_core::mobile::{PowerProfile, NetworkState, AppState};

/// Configuration for adaptive cover traffic generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveCoverConfig {
    /// Base lambda value for Poisson distribution (packets per second)
    pub base_lambda: f64,
    /// Maximum lambda value to prevent excessive traffic
    pub max_lambda: f64,
    /// Minimum lambda value to maintain basic cover
    pub min_lambda: f64,
    /// Target bandwidth utilization (0.0 to 1.0)
    pub target_utilization: f64,
    /// Adaptation speed (0.0 to 1.0, higher = faster adaptation)
    pub adaptation_speed: f64,
    /// Time window for traffic analysis (seconds)
    pub analysis_window: u64,
    /// Enable mobile power state adaptation
    pub mobile_adaptation: bool,
    /// Enable time-based adaptation
    pub time_based_adaptation: bool,
    /// Enable network condition adaptation
    pub network_adaptation: bool,
}

impl Default for AdaptiveCoverConfig {
    fn default() -> Self {
        Self {
            base_lambda: 2.0,           // 2 packets/second base rate
            max_lambda: 10.0,           // Max 10 packets/second
            min_lambda: 0.1,            // Min 0.1 packets/second
            target_utilization: 0.35,   // 35% bandwidth for cover traffic
            adaptation_speed: 0.1,      // Moderate adaptation speed
            analysis_window: 60,        // 1 minute analysis window
            mobile_adaptation: true,
            time_based_adaptation: true,
            network_adaptation: true,
        }
    }
}

/// Real-time metrics for cover traffic generation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CoverTrafficMetrics {
    /// Current lambda value
    pub current_lambda: f64,
    /// Packets generated in last window
    pub packets_generated: u64,
    /// Actual traffic rate (packets/second)
    pub actual_rate: f64,
    /// Target traffic rate (packets/second)
    pub target_rate: f64,
    /// Power profile scaling factor
    pub power_scale: f64,
    /// Network condition scaling factor
    pub network_scale: f64,
    /// Time-based scaling factor
    pub time_scale: f64,
    /// Total bandwidth used (bytes/second)
    pub bandwidth_used: u64,
    /// Adaptation efficiency (0.0 to 1.0)
    pub efficiency: f64,
}

/// Historical data point for traffic analysis.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TrafficSample {
    timestamp: Instant,
    lambda: f64,
    packets_sent: u64,
    bandwidth_used: u64,
    power_state: Option<PowerProfile>,
}

/// Adaptive cover traffic generator with real-time lambda scaling.
pub struct AdaptiveCoverGenerator {
    /// Configuration
    config: Arc<RwLock<AdaptiveCoverConfig>>,
    /// Current lambda value
    current_lambda: Arc<RwLock<f64>>,
    /// Traffic generation metrics
    metrics: Arc<RwLock<CoverTrafficMetrics>>,
    /// Historical traffic samples
    history: Arc<RwLock<VecDeque<TrafficSample>>>,
    /// Metrics broadcast channel
    metrics_tx: broadcast::Sender<CoverTrafficMetrics>,
    /// Lambda change notifications
    lambda_tx: watch::Sender<f64>,
    /// Last packet generation time
    last_packet_time: Arc<RwLock<Instant>>,
    /// Running statistics
    stats: Arc<RwLock<TrafficStats>>,
}

/// Running statistics for traffic analysis.
#[derive(Debug, Clone)]
pub struct TrafficStats {
    total_packets: u64,
    total_bytes: u64,
    total_runtime: Duration,
    average_lambda: f64,
    peak_lambda: f64,
    min_lambda: f64,
    adaptation_count: u64,
}

impl Default for TrafficStats {
    fn default() -> Self {
        Self {
            total_packets: 0,
            total_bytes: 0,
            total_runtime: Duration::ZERO,
            average_lambda: 0.0,
            peak_lambda: 0.0,
            min_lambda: f64::INFINITY,
            adaptation_count: 0,
        }
    }
}

impl AdaptiveCoverGenerator {
    /// Create a new adaptive cover traffic generator.
    pub fn new(config: AdaptiveCoverConfig) -> Self {
        let (metrics_tx, _) = broadcast::channel(32);
        let (lambda_tx, _) = watch::channel(config.base_lambda);

        let initial_metrics = CoverTrafficMetrics {
            current_lambda: config.base_lambda,
            packets_generated: 0,
            actual_rate: 0.0,
            target_rate: config.base_lambda,
            power_scale: 1.0,
            network_scale: 1.0,
            time_scale: 1.0,
            bandwidth_used: 0,
            efficiency: 1.0,
        };

        let base_lambda = config.base_lambda;
        Self {
            config: Arc::new(RwLock::new(config)),
            current_lambda: Arc::new(RwLock::new(base_lambda)),
            metrics: Arc::new(RwLock::new(initial_metrics)),
            history: Arc::new(RwLock::new(VecDeque::new())),
            metrics_tx,
            lambda_tx,
            last_packet_time: Arc::new(RwLock::new(Instant::now())),
            stats: Arc::new(RwLock::new(TrafficStats::default())),
        }
    }

    /// Start the adaptive cover traffic generator.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting adaptive cover traffic generator");

        // Start adaptation engine
        self.start_adaptation_engine().await;

        // Start metrics collection
        self.start_metrics_collection().await;

        // Start traffic generation
        self.start_traffic_generation().await;

        Ok(())
    }

    /// Get current traffic metrics.
    pub async fn metrics(&self) -> CoverTrafficMetrics {
        *self.metrics.read().await
    }

    /// Subscribe to metrics updates.
    pub fn subscribe_metrics(&self) -> broadcast::Receiver<CoverTrafficMetrics> {
        self.metrics_tx.subscribe()
    }

    /// Subscribe to lambda changes.
    pub fn subscribe_lambda(&self) -> watch::Receiver<f64> {
        self.lambda_tx.subscribe()
    }

    /// Update configuration at runtime.
    pub async fn update_config(&self, new_config: AdaptiveCoverConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Cover traffic configuration updated");
    }

    /// Get current lambda value.
    pub async fn current_lambda(&self) -> f64 {
        *self.current_lambda.read().await
    }

    /// Get traffic statistics.
    pub async fn statistics(&self) -> TrafficStats {
        self.stats.read().await.clone()
    }

    /// Start the adaptation engine that adjusts lambda based on conditions.
    async fn start_adaptation_engine(&self) {
        let config = Arc::clone(&self.config);
        let current_lambda = Arc::clone(&self.current_lambda);
        let metrics = Arc::clone(&self.metrics);
        let _history = Arc::clone(&self.history);
        let lambda_tx = self.lambda_tx.clone();
        let metrics_tx = self.metrics_tx.clone();
        let stats = Arc::clone(&self.stats);

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));

            loop {
                interval.tick().await;

                let cfg = config.read().await.clone();
                let mut lambda = current_lambda.write().await;
                let mut metrics_guard = metrics.write().await;
                let mut stats_guard = stats.write().await;

                // Calculate scaling factors
                let power_scale = Self::calculate_power_scale(&cfg).await;
                let network_scale = Self::calculate_network_scale(&cfg).await;
                let time_scale = Self::calculate_time_scale(&cfg).await;

                // Calculate new lambda
                let target_lambda = cfg.base_lambda * power_scale * network_scale * time_scale;
                let clamped_lambda = target_lambda.clamp(cfg.min_lambda, cfg.max_lambda);

                // Apply adaptation speed
                let new_lambda = *lambda + (clamped_lambda - *lambda) * cfg.adaptation_speed;

                if (*lambda - new_lambda).abs() > 0.01 {
                    *lambda = new_lambda;
                    stats_guard.adaptation_count += 1;
                    debug!("Lambda adapted: {:.3} (power: {:.2}, network: {:.2}, time: {:.2})", 
                           new_lambda, power_scale, network_scale, time_scale);
                    let _ = lambda_tx.send(new_lambda);
                }

                // Update metrics
                metrics_guard.current_lambda = *lambda;
                metrics_guard.target_rate = *lambda;
                metrics_guard.power_scale = power_scale;
                metrics_guard.network_scale = network_scale;
                metrics_guard.time_scale = time_scale;

                // Calculate efficiency
                let efficiency = if metrics_guard.target_rate > 0.0 {
                    (metrics_guard.actual_rate / metrics_guard.target_rate).min(1.0)
                } else {
                    1.0
                };
                metrics_guard.efficiency = efficiency;

                // Update statistics
                stats_guard.average_lambda = (stats_guard.average_lambda * (stats_guard.adaptation_count as f64 - 1.0) + *lambda) / stats_guard.adaptation_count as f64;
                stats_guard.peak_lambda = stats_guard.peak_lambda.max(*lambda);
                stats_guard.min_lambda = stats_guard.min_lambda.min(*lambda);

                let _ = metrics_tx.send(*metrics_guard);
            }
        });
    }

    /// Start metrics collection and analysis.
    async fn start_metrics_collection(&self) {
        let history = Arc::clone(&self.history);
        let metrics = Arc::clone(&self.metrics);
        let config = Arc::clone(&self.config);

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                let cfg = config.read().await;
                let window_size = cfg.analysis_window;
                drop(cfg);

                let now = Instant::now();
                let mut history_guard = history.write().await;
                let mut metrics_guard = metrics.write().await;

                // Clean old samples
                while let Some(sample) = history_guard.front() {
                    if now.duration_since(sample.timestamp).as_secs() > window_size {
                        history_guard.pop_front();
                    } else {
                        break;
                    }
                }

                // Calculate metrics from recent history
                if !history_guard.is_empty() {
                    let total_packets: u64 = history_guard.iter().map(|s| s.packets_sent).sum();
                    let total_bandwidth: u64 = history_guard.iter().map(|s| s.bandwidth_used).sum();
                    let window_duration = window_size as f64;

                    metrics_guard.packets_generated = total_packets;
                    metrics_guard.actual_rate = total_packets as f64 / window_duration;
                    metrics_guard.bandwidth_used = (total_bandwidth as f64 / window_duration) as u64;
                }
            }
        });
    }

    /// Start the actual traffic generation loop.
    async fn start_traffic_generation(&self) {
        let current_lambda = Arc::clone(&self.current_lambda);
        let history = Arc::clone(&self.history);
        let last_packet_time = Arc::clone(&self.last_packet_time);
        let stats = Arc::clone(&self.stats);

        tokio::spawn(async move {
            let mut rng = rand::rngs::StdRng::from_entropy();
            let start_time = Instant::now();

            loop {
                let lambda = *current_lambda.read().await;
                
                // Generate next packet delay using Poisson distribution
                let delay = if lambda > 0.0 {
                    let poisson = Poisson::new(lambda).unwrap_or_else(|_| Poisson::new(1.0).unwrap());
                    let samples = poisson.sample(&mut rng);
                    Duration::from_secs_f64(samples.max(0.1))
                } else {
                    Duration::from_secs(10) // Fallback delay
                };

                let next_packet_time = TokioInstant::now() + delay;
                sleep_until(next_packet_time).await;

                // Generate cover packet
                let packet_size = Self::generate_packet_size(&mut rng);
                
                // Update history
                let sample = TrafficSample {
                    timestamp: Instant::now(),
                    lambda,
                    packets_sent: 1,
                    bandwidth_used: packet_size as u64,
                    power_state: Self::get_current_power_profile().await,
                };

                history.write().await.push_back(sample);
                *last_packet_time.write().await = Instant::now();

                // Update statistics
                let mut stats_guard = stats.write().await;
                stats_guard.total_packets += 1;
                stats_guard.total_bytes += packet_size as u64;
                stats_guard.total_runtime = start_time.elapsed();

                trace!("Generated cover packet: {} bytes, lambda: {:.3}", packet_size, lambda);
            }
        });
    }

    /// Calculate power state scaling factor.
    async fn calculate_power_scale(config: &AdaptiveCoverConfig) -> f64 {
        if !config.mobile_adaptation {
            return 1.0;
        }

        #[cfg(feature = "mobile")]
        {
            if let Some(monitor) = nyx_core::mobile::mobile_monitor() {
                let power_state = monitor.power_state().await;
                return power_state.power_profile.cover_traffic_scale();
            }
        }

        1.0
    }

    /// Calculate network condition scaling factor.
    async fn calculate_network_scale(config: &AdaptiveCoverConfig) -> f64 {
        if !config.network_adaptation {
            return 1.0;
        }

        #[cfg(feature = "mobile")]
        {
            if let Some(monitor) = nyx_core::mobile::mobile_monitor() {
                let network_state = monitor.network_state().await;
                return match network_state {
                    NetworkState::WiFi => 1.0,
                    NetworkState::Cellular => 0.6,
                    NetworkState::Ethernet => 1.2,
                    NetworkState::None => 0.0,
                };
            }
        }

        1.0
    }

    /// Calculate time-based scaling factor.
    async fn calculate_time_scale(config: &AdaptiveCoverConfig) -> f64 {
        if !config.time_based_adaptation {
            return 1.0;
        }

        // Get current time of day
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let hour = (now / 3600) % 24;
        
        // Scale based on typical usage patterns
        match hour {
            0..=6 => 0.3,   // Night: reduced activity
            7..=9 => 0.8,   // Morning: moderate activity
            10..=17 => 1.0, // Day: normal activity
            18..=21 => 1.2, // Evening: peak activity
            22..=23 => 0.6, // Late evening: reduced activity
            _ => 1.0,
        }
    }

    /// Get current power profile from mobile monitor.
    async fn get_current_power_profile() -> Option<PowerProfile> {
        #[cfg(feature = "mobile")]
        {
            if let Some(monitor) = nyx_core::mobile::mobile_monitor() {
                let power_state = monitor.power_state().await;
                return Some(power_state.power_profile);
            }
        }
        None
    }

    /// Generate realistic packet size for cover traffic.
    fn generate_packet_size(rng: &mut impl Rng) -> usize {
        // Generate realistic packet sizes based on typical network traffic
        let size_class: f64 = rng.gen();
        
        if size_class < 0.4 {
            // Small packets (40-200 bytes) - 40%
            rng.gen_range(40..=200)
        } else if size_class < 0.7 {
            // Medium packets (200-800 bytes) - 30%
            rng.gen_range(200..=800)
        } else if size_class < 0.9 {
            // Large packets (800-1280 bytes) - 20%
            rng.gen_range(800..=1280)
        } else {
            // Fixed size packets (1280 bytes) - 10%
            1280
        }
    }
}

/// Real-time cover traffic testing framework.
#[allow(dead_code)]
pub struct CoverTrafficTester {
    generator: AdaptiveCoverGenerator,
    test_duration: Duration,
    test_scenarios: Vec<TestScenario>,
}

/// Test scenario for cover traffic validation.
#[derive(Debug, Clone)]
pub struct TestScenario {
    pub name: String,
    pub duration: Duration,
    pub expected_lambda_range: (f64, f64),
    pub power_profile: Option<PowerProfile>,
    pub network_state: Option<NetworkState>,
    pub app_state: Option<AppState>,
}

impl CoverTrafficTester {
    /// Create a new cover traffic tester.
    pub fn new(config: AdaptiveCoverConfig) -> Self {
        let generator = AdaptiveCoverGenerator::new(config);
        
        Self {
            generator,
            test_duration: Duration::from_secs(300), // 5 minutes default
            test_scenarios: Self::default_test_scenarios(),
        }
    }

    /// Run comprehensive cover traffic tests.
    pub async fn run_tests(&self) -> Result<TestResults, Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting cover traffic real-time tests");

        let mut results = TestResults::new();
        
        // Start the generator
        self.generator.start().await?;

        // Run each test scenario
        for scenario in &self.test_scenarios {
            info!("Running test scenario: {}", scenario.name);
            
            let scenario_result = self.run_scenario(scenario).await?;
            results.scenario_results.push(scenario_result);
        }

        // Calculate overall results
        results.calculate_summary();

        info!("Cover traffic tests completed. Overall success rate: {:.1}%", 
              results.overall_success_rate * 100.0);

        Ok(results)
    }

    /// Run a single test scenario.
    async fn run_scenario(&self, scenario: &TestScenario) -> Result<ScenarioResult, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut _lambda_samples = Vec::new();
        let mut metrics_rx = self.generator.subscribe_metrics();
        let scenario_duration = scenario.duration;

        // Simulate scenario conditions
        self.simulate_scenario_conditions(scenario).await;

        // Collect metrics during scenario
        let metrics_task = tokio::spawn(async move {
            let mut samples = Vec::new();
            let end_time = start_time + scenario_duration;
            
            while Instant::now() < end_time {
                if let Ok(metrics) = metrics_rx.recv().await {
                    samples.push(metrics.current_lambda);
                }
            }
            samples
        });

        // Wait for scenario duration
        sleep_until(TokioInstant::now() + scenario_duration).await;

        _lambda_samples = metrics_task.await.unwrap_or_default();

        // Analyze results
        let avg_lambda = _lambda_samples.iter().sum::<f64>() / _lambda_samples.len() as f64;
        let in_range = avg_lambda >= scenario.expected_lambda_range.0 && avg_lambda <= scenario.expected_lambda_range.1;

        Ok(ScenarioResult {
            name: scenario.name.clone(),
            duration: scenario_duration,
            average_lambda: avg_lambda,
            expected_range: scenario.expected_lambda_range,
            lambda_samples: _lambda_samples,
            success: in_range,
            efficiency: self.calculate_scenario_efficiency(scenario, avg_lambda).await,
        })
    }

    /// Simulate scenario-specific conditions.
    async fn simulate_scenario_conditions(&self, _scenario: &TestScenario) {
        // In a real implementation, this would:
        // - Set mock power states
        // - Simulate network conditions
        // - Trigger app state changes
        // For now, we just log the scenario
        debug!("Simulating conditions for scenario: {}", _scenario.name);
    }

    /// Calculate efficiency for a scenario.
    async fn calculate_scenario_efficiency(&self, scenario: &TestScenario, avg_lambda: f64) -> f64 {
        let target_lambda = (scenario.expected_lambda_range.0 + scenario.expected_lambda_range.1) / 2.0;
        if target_lambda > 0.0 {
            1.0 - (avg_lambda - target_lambda).abs() / target_lambda
        } else {
            1.0
        }
    }

    /// Default test scenarios for cover traffic validation.
    fn default_test_scenarios() -> Vec<TestScenario> {
        vec![
            TestScenario {
                name: "High Performance Mode".to_string(),
                duration: Duration::from_secs(60),
                expected_lambda_range: (1.8, 2.2),
                power_profile: Some(PowerProfile::HighPerformance),
                network_state: Some(NetworkState::WiFi),
                app_state: Some(AppState::Active),
            },
            TestScenario {
                name: "Power Saver Mode".to_string(),
                duration: Duration::from_secs(60),
                expected_lambda_range: (0.5, 0.8),
                power_profile: Some(PowerProfile::PowerSaver),
                network_state: Some(NetworkState::WiFi),
                app_state: Some(AppState::Background),
            },
            TestScenario {
                name: "Ultra Low Power Mode".to_string(),
                duration: Duration::from_secs(60),
                expected_lambda_range: (0.1, 0.3),
                power_profile: Some(PowerProfile::UltraLowPower),
                network_state: Some(NetworkState::Cellular),
                app_state: Some(AppState::Background),
            },
            TestScenario {
                name: "Cellular Network".to_string(),
                duration: Duration::from_secs(45),
                expected_lambda_range: (1.0, 1.5),
                power_profile: Some(PowerProfile::Balanced),
                network_state: Some(NetworkState::Cellular),
                app_state: Some(AppState::Active),
            },
            TestScenario {
                name: "No Network".to_string(),
                duration: Duration::from_secs(30),
                expected_lambda_range: (0.0, 0.1),
                power_profile: Some(PowerProfile::Balanced),
                network_state: Some(NetworkState::None),
                app_state: Some(AppState::Active),
            },
        ]
    }
}

/// Results from cover traffic testing.
#[derive(Debug, Clone)]
pub struct TestResults {
    pub scenario_results: Vec<ScenarioResult>,
    pub overall_success_rate: f64,
    pub total_test_time: Duration,
    pub average_efficiency: f64,
}

impl TestResults {
    fn new() -> Self {
        Self {
            scenario_results: Vec::new(),
            overall_success_rate: 0.0,
            total_test_time: Duration::ZERO,
            average_efficiency: 0.0,
        }
    }

    fn calculate_summary(&mut self) {
        let successful = self.scenario_results.iter().filter(|r| r.success).count();
        self.overall_success_rate = successful as f64 / self.scenario_results.len() as f64;
        
        self.total_test_time = self.scenario_results.iter().map(|r| r.duration).sum();
        
        self.average_efficiency = self.scenario_results.iter()
            .map(|r| r.efficiency)
            .sum::<f64>() / self.scenario_results.len() as f64;
    }
}

/// Results from a single test scenario.
#[derive(Debug, Clone)]
pub struct ScenarioResult {
    pub name: String,
    pub duration: Duration,
    pub average_lambda: f64,
    pub expected_range: (f64, f64),
    pub lambda_samples: Vec<f64>,
    pub success: bool,
    pub efficiency: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_adaptive_cover_generator() {
        let config = AdaptiveCoverConfig::default();
        let generator = AdaptiveCoverGenerator::new(config);

        // Test initial state
        let initial_lambda = generator.current_lambda().await;
        assert_eq!(initial_lambda, 2.0);

        // Test metrics
        let metrics = generator.metrics().await;
        assert_eq!(metrics.current_lambda, 2.0);
    }

    #[tokio::test]
    async fn test_power_profile_scaling() {
        assert_eq!(PowerProfile::HighPerformance.cover_traffic_scale(), 1.0);
        assert_eq!(PowerProfile::PowerSaver.cover_traffic_scale(), 0.3);
        assert_eq!(PowerProfile::UltraLowPower.cover_traffic_scale(), 0.1);
    }

    #[tokio::test]
    async fn test_packet_size_generation() {
        let mut rng = thread_rng();
        let size = AdaptiveCoverGenerator::generate_packet_size(&mut rng);
        assert!(size >= 40 && size <= 1280);
    }

    #[tokio::test]
    async fn test_cover_traffic_tester() {
        let config = AdaptiveCoverConfig {
            base_lambda: 1.0,
            max_lambda: 5.0,
            min_lambda: 0.1,
            ..Default::default()
        };
        
        let tester = CoverTrafficTester::new(config);
        assert_eq!(tester.test_scenarios.len(), 5);
    }

    #[tokio::test]
    async fn test_time_based_scaling() {
        let config = AdaptiveCoverConfig::default();
        let scale = AdaptiveCoverGenerator::calculate_time_scale(&config).await;
        assert!(scale > 0.0 && scale <= 1.2);
    }
} 