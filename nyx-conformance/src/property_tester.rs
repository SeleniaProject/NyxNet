//! Property-based testing framework for Nyx protocol
//!
//! This module provides a comprehensive framework for property-based testing
//! of the Nyx protocol, including test case generation, property verification,
//! counterexample generation, and detailed failure reporting.


use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Property test configuration
#[derive(Debug, Clone)]
pub struct PropertyTestConfig {
    /// Number of test iterations to run
    pub iterations: usize,
    /// Random seed for reproducible tests
    pub seed: Option<u64>,
    /// Maximum test case size
    pub max_size: usize,
    /// Shrinking attempts for counterexamples
    pub shrink_attempts: usize,
    /// Timeout for individual test cases
    pub test_timeout: Duration,
    /// Enable detailed logging
    pub verbose: bool,
}

impl Default for PropertyTestConfig {
    fn default() -> Self {
        Self {
            iterations: 100,
            seed: None,
            max_size: 1000,
            shrink_attempts: 100,
            test_timeout: Duration::from_secs(5),
            verbose: false,
        }
    }
}

/// Test case generator trait
pub trait Generator<T>: Send + Sync {
    /// Generate a random test case
    fn generate(&self, rng: &mut StdRng, size: usize) -> T;
    
    /// Shrink a test case to find minimal counterexample
    fn shrink(&self, value: &T) -> Vec<T>;
    
    /// Get the name of this generator
    fn name(&self) -> &'static str;
}

/// Property trait for defining testable properties
pub trait Property<T>: Send + Sync {
    /// Test the property against a generated value
    fn test(&self, value: &T) -> PropertyResult;
    
    /// Get the name of this property
    fn name(&self) -> &'static str;
    
    /// Get a description of what this property tests
    fn description(&self) -> &'static str;
}

/// Result of a property test
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyResult {
    /// Property passed
    Passed,
    /// Property failed with error message
    Failed(String),
    /// Property test was discarded (e.g., invalid input)
    Discarded,
}

/// Test case with metadata
#[derive(Debug, Clone)]
pub struct TestCase<T> {
    /// The generated test value
    pub value: T,
    /// Size parameter used for generation
    pub size: usize,
    /// Random seed used for this case
    pub seed: u64,
    /// Generation time
    pub generated_at: Instant,
}

/// Counterexample with shrinking information
#[derive(Debug, Clone)]
pub struct CounterExample<T> {
    /// The minimal failing test case
    pub minimal_case: TestCase<T>,
    /// Original failing test case
    pub original_case: TestCase<T>,
    /// Shrinking steps taken
    pub shrink_steps: Vec<TestCase<T>>,
    /// Property that failed
    pub property_name: String,
    /// Failure message
    pub failure_message: String,
}

/// Test results summary
#[derive(Debug, Clone)]
pub struct TestResults<T> {
    /// Property that was tested
    pub property_name: String,
    /// Total test cases run
    pub total_cases: usize,
    /// Passed test cases
    pub passed_cases: usize,
    /// Failed test cases
    pub failed_cases: usize,
    /// Discarded test cases
    pub discarded_cases: usize,
    /// Test execution time
    pub execution_time: Duration,
    /// Counterexample if any
    pub counterexample: Option<CounterExample<T>>,
    /// Success rate
    pub success_rate: f64,
}

/// Test execution record
#[derive(Debug, Clone)]
struct TestExecution {
    property_name: String,
    generator_name: String,
    start_time: Instant,
    end_time: Instant,
    result: PropertyResult,
    test_case_size: usize,
}

/// Main property tester for a specific type
pub struct PropertyTester<T> {
    config: PropertyTestConfig,
    generator: Box<dyn Generator<T>>,
    properties: Vec<Box<dyn Property<T>>>,
    rng: StdRng,
    test_history: Arc<Mutex<Vec<TestExecution>>>,
}

impl<T: Clone + Debug + 'static> PropertyTester<T> {
    /// Create a new property tester with the given configuration and generator
    pub fn new(config: PropertyTestConfig, generator: Box<dyn Generator<T>>) -> Self {
        let seed = config.seed.unwrap_or_else(|| {
            use std::time::SystemTime;
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });
        
        Self {
            config,
            generator,
            properties: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
            test_history: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Add a property to test
    pub fn add_property(&mut self, property: Box<dyn Property<T>>) {
        self.properties.push(property);
    }
    
    /// Run all registered properties
    pub fn run_all_tests(&mut self) -> Vec<TestResults<T>> {
        let mut results = Vec::new();
        
        // Clone property names to avoid borrowing issues
        let property_count = self.properties.len();
        for i in 0..property_count {
            let result = self.run_property_test_by_index(i);
            results.push(result);
        }
        
        results
    }
    
    /// Run a property test by index
    fn run_property_test_by_index(&mut self, index: usize) -> TestResults<T> {
        let property_name = self.properties[index].name().to_string();
        let _property_description = self.properties[index].description().to_string();
        
        let start_time = Instant::now();
        let mut passed = 0;
        let mut failed = 0;
        let mut discarded = 0;
        let mut counterexample = None;
        
        for i in 0..self.config.iterations {
            let size = std::cmp::min(i / 10, self.config.max_size);
            let test_case_value = self.generator.generate(&mut self.rng, size);
            
            let test_case = TestCase {
                value: test_case_value,
                size,
                seed: self.rng.gen(),
                generated_at: Instant::now(),
            };
            
            let test_start = Instant::now();
            let result = self.properties[index].test(&test_case.value);
            let test_end = Instant::now();
            
            // Record test execution
            {
                let mut history = self.test_history.lock().unwrap();
                history.push(TestExecution {
                    property_name: property_name.clone(),
                    generator_name: self.generator.name().to_string(),
                    start_time: test_start,
                    end_time: test_end,
                    result: result.clone(),
                    test_case_size: size,
                });
            }
            
            match result {
                PropertyResult::Passed => passed += 1,
                PropertyResult::Failed(msg) => {
                    failed += 1;
                    if counterexample.is_none() {
                        // Try to shrink the counterexample
                        let shrunk = self.shrink_counterexample_by_index(test_case, index, msg);
                        counterexample = Some(shrunk);
                    }
                    break; // Stop on first failure
                },
                PropertyResult::Discarded => discarded += 1,
            }
        }
        
        let execution_time = start_time.elapsed();
        let total_cases = passed + failed + discarded;
        let success_rate = if total_cases > 0 {
            passed as f64 / total_cases as f64
        } else {
            0.0
        };
        
        TestResults {
            property_name,
            total_cases,
            passed_cases: passed,
            failed_cases: failed,
            discarded_cases: discarded,
            execution_time,
            counterexample,
            success_rate,
        }
    }
    

    
    /// Shrink a counterexample to find minimal failing case
    fn shrink_counterexample_by_index(
        &mut self,
        original_case: TestCase<T>,
        property_index: usize,
        failure_message: String,
    ) -> CounterExample<T> {
        let mut current_case = original_case.clone();
        let mut shrink_steps = Vec::new();
        let property_name = self.properties[property_index].name().to_string();
        
        for _ in 0..self.config.shrink_attempts {
            let candidates = self.generator.shrink(&current_case.value);
            
            let mut found_smaller = false;
            for candidate in candidates {
                match self.properties[property_index].test(&candidate) {
                    PropertyResult::Failed(_) => {
                        // Found a smaller failing case
                        let shrunk_case = TestCase {
                            value: candidate,
                            size: current_case.size,
                            seed: 0, // Shrunk cases don't have seeds
                            generated_at: Instant::now(),
                        };
                        shrink_steps.push(current_case.clone());
                        current_case = shrunk_case;
                        found_smaller = true;
                        break;
                    },
                    _ => continue,
                }
            }
            
            if !found_smaller {
                break;
            }
        }
        
        CounterExample {
            minimal_case: current_case,
            original_case: original_case,
            shrink_steps,
            property_name,
            failure_message,
        }
    }
    
    /// Generate a detailed test report
    pub fn generate_report(&self, results: &[TestResults<T>]) -> String {
        let mut report = String::new();
        report.push_str("=== Property Test Report ===\n\n");
        
        let total_properties = results.len();
        let passed_properties = results.iter().filter(|r| r.failed_cases == 0).count();
        let failed_properties = total_properties - passed_properties;
        
        report.push_str(&format!("Properties tested: {}\n", total_properties));
        report.push_str(&format!("Properties passed: {}\n", passed_properties));
        report.push_str(&format!("Properties failed: {}\n", failed_properties));
        report.push_str("\n");
        
        for result in results {
            report.push_str(&format!("Property: {}\n", result.property_name));
            report.push_str(&format!("  Total cases: {}\n", result.total_cases));
            report.push_str(&format!("  Passed: {}\n", result.passed_cases));
            report.push_str(&format!("  Failed: {}\n", result.failed_cases));
            report.push_str(&format!("  Discarded: {}\n", result.discarded_cases));
            report.push_str(&format!("  Success rate: {:.2}%\n", result.success_rate * 100.0));
            report.push_str(&format!("  Execution time: {:?}\n", result.execution_time));
            
            if let Some(ref counterexample) = result.counterexample {
                report.push_str("  COUNTEREXAMPLE FOUND:\n");
                report.push_str(&format!("    Property: {}\n", counterexample.property_name));
                report.push_str(&format!("    Failure: {}\n", counterexample.failure_message));
                report.push_str(&format!("    Shrink steps: {}\n", counterexample.shrink_steps.len()));
                report.push_str(&format!("    Minimal case: {:?}\n", counterexample.minimal_case.value));
            }
            
            report.push_str("\n");
        }
        
        report
    }
    
    /// Get test execution history
    pub fn get_test_history(&self) -> Vec<TestExecution> {
        self.test_history.lock().unwrap().clone()
    }
    
    /// Clear test history
    pub fn clear_history(&self) {
        self.test_history.lock().unwrap().clear();
    }
}

// Built-in generators

/// Generator for u32 values
pub struct U32Generator {
    min: u32,
    max: u32,
}

impl U32Generator {
    pub fn new(min: u32, max: u32) -> Self {
        Self { min, max }
    }
}

impl Generator<u32> for U32Generator {
    fn generate(&self, rng: &mut StdRng, _size: usize) -> u32 {
        rng.gen_range(self.min..=self.max)
    }
    
    fn shrink(&self, value: &u32) -> Vec<u32> {
        let mut shrunk = Vec::new();
        
        // Try smaller values
        if *value > self.min {
            shrunk.push(self.min);
            if *value > self.min + 1 {
                shrunk.push(*value / 2);
                shrunk.push(*value - 1);
            }
        }
        
        shrunk
    }
    
    fn name(&self) -> &'static str {
        "U32Generator"
    }
}

/// Generator for Vec<u8> values
pub struct ByteVecGenerator {
    min_len: usize,
    max_len: usize,
}

impl ByteVecGenerator {
    pub fn new(min_len: usize, max_len: usize) -> Self {
        Self { min_len, max_len }
    }
}

impl Generator<Vec<u8>> for ByteVecGenerator {
    fn generate(&self, rng: &mut StdRng, size: usize) -> Vec<u8> {
        let len = rng.gen_range(self.min_len..=std::cmp::min(self.max_len, size));
        (0..len).map(|_| rng.gen()).collect()
    }
    
    fn shrink(&self, value: &Vec<u8>) -> Vec<Vec<u8>> {
        let mut shrunk = Vec::new();
        
        // Try empty vector
        if value.len() > self.min_len {
            shrunk.push(vec![]);
        }
        
        // Try shorter vectors
        if value.len() > 1 {
            shrunk.push(value[..value.len() / 2].to_vec());
            shrunk.push(value[..value.len() - 1].to_vec());
        }
        
        // Try simpler byte values
        if !value.is_empty() {
            let mut simplified = value.clone();
            simplified[0] = 0;
            shrunk.push(simplified);
        }
        
        shrunk
    }
    
    fn name(&self) -> &'static str {
        "ByteVecGenerator"
    }
}

/// Generator for Duration values
pub struct DurationGenerator {
    min_ms: u64,
    max_ms: u64,
}

impl DurationGenerator {
    pub fn new(min_ms: u64, max_ms: u64) -> Self {
        Self { min_ms, max_ms }
    }
}

impl Generator<Duration> for DurationGenerator {
    fn generate(&self, rng: &mut StdRng, _size: usize) -> Duration {
        let ms = rng.gen_range(self.min_ms..=self.max_ms);
        Duration::from_millis(ms)
    }
    
    fn shrink(&self, value: &Duration) -> Vec<Duration> {
        let mut shrunk = Vec::new();
        let ms = value.as_millis() as u64;
        
        if ms > self.min_ms {
            shrunk.push(Duration::from_millis(self.min_ms));
            if ms > self.min_ms + 1 {
                shrunk.push(Duration::from_millis(ms / 2));
                shrunk.push(Duration::from_millis(ms - 1));
            }
        }
        
        shrunk
    }
    
    fn name(&self) -> &'static str {
        "DurationGenerator"
    }
}

// Built-in properties

/// Property that checks if a value is within bounds
pub struct BoundsProperty<T> {
    min: T,
    max: T,
    name: &'static str,
}

impl<T: PartialOrd + Clone + Debug> BoundsProperty<T> {
    pub fn new(min: T, max: T, name: &'static str) -> Self {
        Self { min, max, name }
    }
}

impl<T: PartialOrd + Clone + Debug + Send + Sync> Property<T> for BoundsProperty<T> {
    fn test(&self, value: &T) -> PropertyResult {
        if value >= &self.min && value <= &self.max {
            PropertyResult::Passed
        } else {
            PropertyResult::Failed(format!(
                "Value {:?} is not within bounds [{:?}, {:?}]",
                value, self.min, self.max
            ))
        }
    }
    
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn description(&self) -> &'static str {
        "Checks if value is within specified bounds"
    }
}

/// Property that checks if a collection is not empty
pub struct NonEmptyProperty {
    name: &'static str,
}

impl NonEmptyProperty {
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl<T: Send + Sync> Property<Vec<T>> for NonEmptyProperty {
    fn test(&self, value: &Vec<T>) -> PropertyResult {
        if value.is_empty() {
            PropertyResult::Failed("Vector should not be empty".to_string())
        } else {
            PropertyResult::Passed
        }
    }
    
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn description(&self) -> &'static str {
        "Checks if collection is not empty"
    }
}

/// Property that checks if a value satisfies a custom predicate
pub struct PredicateProperty<T> {
    predicate: Box<dyn Fn(&T) -> bool + Send + Sync>,
    name: &'static str,
    description: &'static str,
}

impl<T> PredicateProperty<T> {
    pub fn new<F>(predicate: F, name: &'static str, description: &'static str) -> Self 
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        Self {
            predicate: Box::new(predicate),
            name,
            description,
        }
    }
}

impl<T: Debug + Send + Sync> Property<T> for PredicateProperty<T> {
    fn test(&self, value: &T) -> PropertyResult {
        if (self.predicate)(value) {
            PropertyResult::Passed
        } else {
            PropertyResult::Failed(format!("Predicate failed for value: {:?}", value))
        }
    }
    
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn description(&self) -> &'static str {
        self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u32_generator() {
        let mut rng = StdRng::seed_from_u64(42);
        let gen = U32Generator::new(10, 100);
        
        for _ in 0..100 {
            let value = gen.generate(&mut rng, 50);
            assert!(value >= 10 && value <= 100);
        }
    }

    #[test]
    fn test_bounds_property() {
        let prop = BoundsProperty::new(0u32, 100u32, "test_bounds");
        
        assert_eq!(prop.test(&50), PropertyResult::Passed);
        assert_eq!(prop.test(&0), PropertyResult::Passed);
        assert_eq!(prop.test(&100), PropertyResult::Passed);
        
        match prop.test(&150) {
            PropertyResult::Failed(_) => {},
            _ => panic!("Expected failure for out-of-bounds value"),
        }
    }

    #[test]
    fn test_property_tester() {
        let config = PropertyTestConfig {
            iterations: 10,
            seed: Some(42),
            ..Default::default()
        };
        
        let generator = Box::new(U32Generator::new(0, 50));
        let mut tester = PropertyTester::new(config, generator);
        
        tester.add_property(Box::new(BoundsProperty::new(0u32, 100u32, "bounds_check")));
        
        let results = tester.run_all_tests();
        assert_eq!(results.len(), 1);
        
        let result = &results[0];
        assert_eq!(result.property_name, "bounds_check");
        assert!(result.success_rate > 0.0);
    }

    #[test]
    fn test_shrinking() {
        let gen = U32Generator::new(0, 1000);
        let shrunk = gen.shrink(&500);
        
        assert!(!shrunk.is_empty());
        assert!(shrunk.contains(&0)); // Should try minimum
        assert!(shrunk.contains(&250)); // Should try half
        assert!(shrunk.contains(&499)); // Should try value - 1
    }

    #[test]
    fn test_byte_vec_generator() {
        let mut rng = StdRng::seed_from_u64(42);
        let gen = ByteVecGenerator::new(1, 10);
        
        for _ in 0..50 {
            let value = gen.generate(&mut rng, 20);
            assert!(value.len() >= 1 && value.len() <= 10);
        }
    }

    #[test]
    fn test_predicate_property() {
        let prop = PredicateProperty::new(
            |x: &u32| *x % 2 == 0,
            "even_check",
            "Checks if number is even"
        );
        
        assert_eq!(prop.test(&4), PropertyResult::Passed);
        assert_eq!(prop.test(&6), PropertyResult::Passed);
        
        match prop.test(&5) {
            PropertyResult::Failed(_) => {},
            _ => panic!("Expected failure for odd number"),
        }
    }
}