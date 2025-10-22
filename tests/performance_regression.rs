//! Performance regression detection framework for TTT
//!
//! Provides automated benchmarking and regression detection to ensure
//! that optimizations improve performance without breaking correctness.

use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use ttt::core::{Term, Value, Level};
use ttt::typeck::{Context, infer, check};
use ttt::eval::{normalize, evaluate, convertible};
use ttt::core::subst::{apply_substitution, Substitution};

/// Performance metrics for a specific operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub operation: String,
    pub input_size: usize,
    pub avg_duration_ns: u64,
    pub min_duration_ns: u64,
    pub max_duration_ns: u64,
    pub samples: usize,
    pub memory_estimate: Option<usize>,
}

/// Performance baseline for regression detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    pub version: String,
    pub metrics: HashMap<String, PerformanceMetrics>,
    pub timestamp: String,
}

/// Performance regression detector
pub struct RegressionDetector {
    baseline: Option<PerformanceBaseline>,
    tolerance_factor: f64, // Allow 20% performance degradation by default
}

impl RegressionDetector {
    pub fn new() -> Self {
        Self {
            baseline: None,
            tolerance_factor: 1.2,
        }
    }

    pub fn with_tolerance(mut self, factor: f64) -> Self {
        self.tolerance_factor = factor;
        self
    }

    pub fn load_baseline(&mut self, baseline: PerformanceBaseline) {
        self.baseline = Some(baseline);
    }

    pub fn benchmark_operation<F, R>(&self, name: &str, input_size: usize, samples: usize, operation: F) -> PerformanceMetrics
    where
        F: Fn() -> R,
    {
        let mut durations = Vec::new();

        // Warmup
        for _ in 0..std::cmp::min(samples / 4, 10) {
            let _ = operation();
        }

        // Actual measurements
        for _ in 0..samples {
            let start = Instant::now();
            let _ = operation();
            let duration = start.elapsed();
            durations.push(duration.as_nanos() as u64);
        }

        let min_duration = *durations.iter().min().unwrap();
        let max_duration = *durations.iter().max().unwrap();
        let avg_duration = durations.iter().sum::<u64>() / samples as u64;

        PerformanceMetrics {
            operation: name.to_string(),
            input_size,
            avg_duration_ns: avg_duration,
            min_duration_ns: min_duration,
            max_duration_ns: max_duration,
            samples,
            memory_estimate: None,
        }
    }

    pub fn check_regression(&self, current: &PerformanceMetrics) -> Result<(), String> {
        if let Some(baseline) = &self.baseline {
            if let Some(baseline_metric) = baseline.metrics.get(&current.operation) {
                let baseline_avg = baseline_metric.avg_duration_ns as f64;
                let current_avg = current.avg_duration_ns as f64;
                let ratio = current_avg / baseline_avg;

                if ratio > self.tolerance_factor {
                    return Err(format!(
                        "Performance regression detected for {}: {:.2}x slower than baseline ({}ns vs {}ns)",
                        current.operation,
                        ratio,
                        current.avg_duration_ns,
                        baseline_metric.avg_duration_ns
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Comprehensive performance test suite
pub struct PerformanceTestSuite {
    detector: RegressionDetector,
}

impl PerformanceTestSuite {
    pub fn new() -> Self {
        Self {
            detector: RegressionDetector::new(),
        }
    }

    /// Run all performance benchmarks
    pub fn run_all_benchmarks(&mut self) -> HashMap<String, PerformanceMetrics> {
        let mut results = HashMap::new();

        // Type checking benchmarks
        results.extend(self.type_checking_benchmarks());

        // Normalization benchmarks
        results.extend(self.normalization_benchmarks());

        // Substitution benchmarks
        results.extend(self.substitution_benchmarks());

        // Conversion benchmarks
        results.extend(self.conversion_benchmarks());

        // Complex workflow benchmarks
        results.extend(self.workflow_benchmarks());

        results
    }

    fn type_checking_benchmarks(&self) -> HashMap<String, PerformanceMetrics> {
        let mut results = HashMap::new();
        let context = Context::empty();

        // Simple universe type checking
        let universe_metric = self.detector.benchmark_operation(
            "type_check_universe",
            1,
            1000,
            || {
                let term = Term::universe(0);
                infer(&term, &context)
            }
        );
        results.insert("type_check_universe".to_string(), universe_metric);

        // Variable type checking with context
        let mut ctx_with_var = context.clone();
        ctx_with_var = ctx_with_var.extend("x".to_string(), Value::universe(Level::TYPE));

        let variable_metric = self.detector.benchmark_operation(
            "type_check_variable",
            1,
            1000,
            || {
                let term = Term::var(0);
                infer(&term, &ctx_with_var)
            }
        );
        results.insert("type_check_variable".to_string(), variable_metric);

        // Pi type formation
        let pi_metric = self.detector.benchmark_operation(
            "type_check_pi",
            2,
            500,
            || {
                let domain = Term::universe(0);
                let codomain = Term::universe(0);
                let pi_term = Term::pi(domain, codomain);
                infer(&pi_term, &context)
            }
        );
        results.insert("type_check_pi".to_string(), pi_metric);

        // Lambda type checking
        let lambda_metric = self.detector.benchmark_operation(
            "type_check_lambda",
            2,
            500,
            || {
                let lambda = Term::lambda(Term::var(0));
                let pi_type = Value::pi(
                    Value::universe(Level::TYPE),
                    ttt::core::Closure::empty(Term::var(0))
                );
                check(&lambda, &pi_type, &context)
            }
        );
        results.insert("type_check_lambda".to_string(), lambda_metric);

        // Complex application type checking
        let app_metric = self.detector.benchmark_operation(
            "type_check_application",
            3,
            300,
            || {
                let fun = Term::lambda(Term::var(0));
                let arg = Term::universe(0);
                let app = Term::app(fun, arg);
                infer(&app, &context)
            }
        );
        results.insert("type_check_application".to_string(), app_metric);

        results
    }

    fn normalization_benchmarks(&self) -> HashMap<String, PerformanceMetrics> {
        let mut results = HashMap::new();

        // Simple term normalization
        let simple_metric = self.detector.benchmark_operation(
            "normalize_universe",
            1,
            1000,
            || {
                let term = Term::universe(0);
                normalize(&term)
            }
        );
        results.insert("normalize_universe".to_string(), simple_metric);

        // Lambda normalization
        let lambda_metric = self.detector.benchmark_operation(
            "normalize_lambda",
            2,
            500,
            || {
                let lambda = Term::lambda(Term::var(0));
                normalize(&lambda)
            }
        );
        results.insert("normalize_lambda".to_string(), lambda_metric);

        // Beta reduction
        let beta_metric = self.detector.benchmark_operation(
            "normalize_beta_reduction",
            3,
            300,
            || {
                let identity = Term::lambda(Term::var(0));
                let arg = Term::universe(0);
                let app = Term::app(identity, arg);
                normalize(&app)
            }
        );
        results.insert("normalize_beta_reduction".to_string(), beta_metric);

        // Complex nested terms
        let complex_metric = self.detector.benchmark_operation(
            "normalize_complex_term",
            10,
            100,
            || {
                // Create a complex nested term
                let mut term = Term::var(0);
                for i in 1..10 {
                    let lambda = Term::lambda(term);
                    let arg = Term::var(i);
                    term = Term::app(lambda, arg);
                }
                normalize(&term)
            }
        );
        results.insert("normalize_complex_term".to_string(), complex_metric);

        results
    }

    fn substitution_benchmarks(&self) -> HashMap<String, PerformanceMetrics> {
        let mut results = HashMap::new();

        // Simple substitution
        let simple_metric = self.detector.benchmark_operation(
            "substitute_simple",
            2,
            1000,
            || {
                let term = Term::var(0);
                let replacement = Term::universe(0);
                let subst = Substitution::single(0, replacement);
                apply_substitution(&term, &subst)
            }
        );
        results.insert("substitute_simple".to_string(), simple_metric);

        // Substitution in lambda
        let lambda_metric = self.detector.benchmark_operation(
            "substitute_lambda",
            3,
            500,
            || {
                let lambda = Term::lambda(Term::var(1)); // λx.y
                let replacement = Term::universe(0);
                let subst = Substitution::single(0, replacement);
                apply_substitution(&lambda, &subst)
            }
        );
        results.insert("substitute_lambda".to_string(), lambda_metric);

        // Complex substitution
        let complex_metric = self.detector.benchmark_operation(
            "substitute_complex",
            8,
            200,
            || {
                // Create complex term with multiple variables
                let mut term = Term::var(0);
                for i in 1..8 {
                    term = Term::app(term, Term::var(i));
                }

                // Multiple substitutions
                let mut subst = Substitution::empty();
                for i in 0..4 {
                    subst = subst.extend(i, Term::universe(i as u32));
                }

                apply_substitution(&term, &subst)
            }
        );
        results.insert("substitute_complex".to_string(), complex_metric);

        results
    }

    fn conversion_benchmarks(&self) -> HashMap<String, PerformanceMetrics> {
        let mut results = HashMap::new();
        let env = ttt::core::Environment::new();

        // Simple conversion
        let simple_metric = self.detector.benchmark_operation(
            "convert_identical",
            1,
            1000,
            || {
                let val = Value::universe(Level::TYPE);
                convertible(&val, &val, 0)
            }
        );
        results.insert("convert_identical".to_string(), simple_metric);

        // Universe conversion
        let universe_metric = self.detector.benchmark_operation(
            "convert_universes",
            2,
            500,
            || {
                let val1 = Value::universe(Level(0));
                let val2 = Value::universe(Level(1));
                convertible(&val1, &val2, 0)
            }
        );
        results.insert("convert_universes".to_string(), universe_metric);

        // Lambda conversion
        let lambda_metric = self.detector.benchmark_operation(
            "convert_lambdas",
            3,
            300,
            || {
                let closure1 = ttt::core::Closure::empty(Term::var(0));
                let closure2 = ttt::core::Closure::empty(Term::var(0));
                let lambda1 = Value::lambda(closure1);
                let lambda2 = Value::lambda(closure2);
                convertible(&lambda1, &lambda2, 0)
            }
        );
        results.insert("convert_lambdas".to_string(), lambda_metric);

        results
    }

    fn workflow_benchmarks(&self) -> HashMap<String, PerformanceMetrics> {
        let mut results = HashMap::new();
        let context = Context::empty();

        // End-to-end: type check + normalize
        let e2e_metric = self.detector.benchmark_operation(
            "e2e_type_check_normalize",
            3,
            200,
            || {
                let term = Term::app(
                    Term::lambda(Term::var(0)),
                    Term::universe(0)
                );

                let type_result = infer(&term, &context);
                let norm_result = normalize(&term);
                (type_result, norm_result)
            }
        );
        results.insert("e2e_type_check_normalize".to_string(), e2e_metric);

        // Complex Pi type workflow
        let pi_workflow_metric = self.detector.benchmark_operation(
            "workflow_pi_type_formation",
            5,
            100,
            || {
                // Create Π(A:Type₀).A → A
                let type_0 = Term::universe(0);
                let arrow = Term::pi(Term::var(0), Term::var(1));
                let pi_type = Term::pi(type_0, arrow);

                let type_result = infer(&pi_type, &context);
                let norm_result = normalize(&pi_type);
                (type_result, norm_result)
            }
        );
        results.insert("workflow_pi_type_formation".to_string(), pi_workflow_metric);

        results
    }

    pub fn check_all_regressions(&self, results: &HashMap<String, PerformanceMetrics>) -> Vec<String> {
        let mut regressions = Vec::new();

        for (name, metric) in results {
            if let Err(error) = self.detector.check_regression(metric) {
                regressions.push(error);
            }
        }

        regressions
    }

    pub fn generate_performance_report(&self, results: &HashMap<String, PerformanceMetrics>) -> String {
        let mut report = String::new();
        report.push_str("TTT Performance Report\n");
        report.push_str("=====================\n\n");

        // Group by category
        let mut categories: HashMap<&str, Vec<(&String, &PerformanceMetrics)>> = HashMap::new();

        for (name, metric) in results {
            let category = if name.starts_with("type_check") {
                "Type Checking"
            } else if name.starts_with("normalize") {
                "Normalization"
            } else if name.starts_with("substitute") {
                "Substitution"
            } else if name.starts_with("convert") {
                "Conversion"
            } else if name.starts_with("workflow") || name.starts_with("e2e") {
                "End-to-End Workflows"
            } else {
                "Other"
            };

            categories.entry(category).or_insert_with(Vec::new).push((name, metric));
        }

        for (category, metrics) in categories {
            report.push_str(&format!("{}\n", category));
            report.push_str(&"-".repeat(category.len()));
            report.push_str("\n");

            for (name, metric) in metrics {
                report.push_str(&format!(
                    "{}: {:.2}ms (min: {:.2}ms, max: {:.2}ms, samples: {})\n",
                    name,
                    metric.avg_duration_ns as f64 / 1_000_000.0,
                    metric.min_duration_ns as f64 / 1_000_000.0,
                    metric.max_duration_ns as f64 / 1_000_000.0,
                    metric.samples
                ));
            }
            report.push_str("\n");
        }

        // Check for regressions
        let regressions = self.check_all_regressions(results);
        if !regressions.is_empty() {
            report.push_str("Performance Regressions Detected\n");
            report.push_str("================================\n");
            for regression in regressions {
                report.push_str(&format!("⚠️  {}\n", regression));
            }
            report.push_str("\n");
        } else {
            report.push_str("✅ No performance regressions detected\n\n");
        }

        report
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn run_performance_regression_suite() {
        let mut suite = PerformanceTestSuite::new();
        let results = suite.run_all_benchmarks();

        // Generate report
        let report = suite.generate_performance_report(&results);
        println!("{}", report);

        // Check for major performance issues (very loose bounds for CI)
        for (name, metric) in &results {
            assert!(
                metric.avg_duration_ns < 10_000_000, // 10ms max
                "Operation {} took too long: {}ns",
                name,
                metric.avg_duration_ns
            );
        }

        // Ensure all operations completed successfully
        assert!(!results.is_empty(), "No benchmarks were run");
    }

    #[test]
    fn benchmark_scaling_characteristics() {
        let detector = RegressionDetector::new();

        // Test that operations scale reasonably with input size
        let small_term = Term::universe(0);
        let large_term = {
            let mut term = Term::var(0);
            for i in 1..20 {
                term = Term::lambda(term);
            }
            term
        };

        let context = Context::empty();

        let small_metric = detector.benchmark_operation(
            "scaling_test_small",
            1,
            100,
            || infer(&small_term, &context)
        );

        let large_metric = detector.benchmark_operation(
            "scaling_test_large",
            20,
            100,
            || infer(&large_term, &context)
        );

        // Large operations should not be excessively slower
        let ratio = large_metric.avg_duration_ns as f64 / small_metric.avg_duration_ns as f64;
        assert!(ratio < 100.0, "Large terms scale too poorly: {}x slower", ratio);
    }

    #[test]
    fn memory_usage_estimation() {
        // This test would require more sophisticated memory tracking
        // For now, just ensure operations don't cause obvious memory leaks

        let context = Context::empty();

        for _ in 0..1000 {
            let term = Term::app(
                Term::lambda(Term::var(0)),
                Term::universe(0)
            );

            let _type_result = infer(&term, &context);
            let _norm_result = normalize(&term);
        }

        // If we complete without OOM, the test passes
    }
}

/// Utility functions for performance monitoring
impl PerformanceMetrics {
    pub fn performance_score(&self) -> f64 {
        // Simple score based on average duration and consistency
        let consistency = 1.0 - (self.max_duration_ns - self.min_duration_ns) as f64 / self.avg_duration_ns as f64;
        let speed_score = 1.0 / (self.avg_duration_ns as f64 / 1_000_000.0); // Inverse of milliseconds

        speed_score * consistency.max(0.1) // Don't penalize too heavily for variance
    }

    pub fn is_fast(&self) -> bool {
        self.avg_duration_ns < 1_000_000 // Under 1ms
    }

    pub fn is_consistent(&self) -> bool {
        let variance_ratio = (self.max_duration_ns - self.min_duration_ns) as f64 / self.avg_duration_ns as f64;
        variance_ratio < 0.5 // Max is less than 50% more than average
    }
}