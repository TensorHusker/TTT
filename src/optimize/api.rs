//! High-level optimization API
//!
//! This module provides a simple, safe interface to the TTT optimization system,
//! allowing users to easily enable and configure performance optimizations.

use crate::core::Term;
use crate::eval::normalize as standard_normalize;
use super::{OptimizationConfig, PerformanceMetrics};

/// High-level optimization context
pub struct OptimizedTTT {
    config: OptimizationConfig,
    metrics: PerformanceMetrics,
}

impl OptimizedTTT {
    /// Create a new optimized TTT instance
    pub fn new(config: OptimizationConfig) -> Self {
        OptimizedTTT {
            config,
            metrics: PerformanceMetrics::new(),
        }
    }

    /// Create TTT with default optimizations
    pub fn with_defaults() -> Self {
        Self::new(OptimizationConfig::default())
    }

    /// Create TTT optimized for throughput
    pub fn for_throughput() -> Self {
        Self::new(OptimizationConfig::throughput())
    }

    /// Create TTT optimized for low memory usage
    pub fn for_low_memory() -> Self {
        Self::new(OptimizationConfig::low_memory())
    }

    /// Create TTT optimized for low latency
    pub fn for_low_latency() -> Self {
        Self::new(OptimizationConfig::low_latency())
    }

    /// Normalize a term with optimizations
    pub fn normalize(&mut self, term: &Term) -> Result<Term, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();

        let result = if self.config.enable_fusion {
            // Use fusion-optimized normalization
            self.normalize_with_fusion(term)
        } else {
            // Use standard normalization
            standard_normalize(term).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        };

        let elapsed = start.elapsed().as_nanos() as u64;
        self.metrics.normalization_time_ns += elapsed;

        result
    }

    /// Normalize multiple terms in parallel (if enabled)
    pub fn normalize_batch(&mut self, terms: &[Term]) -> Vec<Result<Term, Box<dyn std::error::Error>>> {
        if self.config.enable_parallel && terms.len() > 1 {
            self.normalize_parallel_batch(terms)
        } else {
            // Sequential normalization
            terms.iter().map(|term| self.normalize(term)).collect()
        }
    }

    /// Check if two terms are convertible with optimizations
    pub fn convertible(&mut self, term1: &Term, term2: &Term) -> bool {
        let start = std::time::Instant::now();

        let result = if self.config.enable_hash_consing {
            // Use hash-consed comparison for potential speedup
            self.convertible_with_hash_consing(term1, term2)
        } else {
            // Use standard convertibility
            crate::eval::convertible(term1, term2)
        };

        let elapsed = start.elapsed().as_nanos() as u64;
        self.metrics.conversion_time_ns += elapsed;

        result
    }

    /// Apply substitution with optimizations
    pub fn substitute(&mut self, term: &Term, substitution: &crate::core::subst::Substitution) -> Term {
        let start = std::time::Instant::now();

        let result = if self.config.enable_fusion {
            // Use fusion-optimized substitution
            super::substitute_vectorized(term, substitution)
        } else {
            // Use standard substitution
            crate::core::subst::apply_substitution(term, substitution)
        };

        let elapsed = start.elapsed().as_nanos() as u64;
        self.metrics.substitution_time_ns += elapsed;

        result
    }

    /// Get current performance metrics
    pub fn metrics(&self) -> &PerformanceMetrics {
        &self.metrics
    }

    /// Reset performance metrics
    pub fn reset_metrics(&mut self) {
        self.metrics.reset();
    }

    /// Get optimization configuration
    pub fn config(&self) -> &OptimizationConfig {
        &self.config
    }

    /// Update optimization configuration
    pub fn set_config(&mut self, config: OptimizationConfig) {
        self.config = config;
    }

    // Private implementation methods

    fn normalize_with_fusion(&mut self, term: &Term) -> Result<Term, Box<dyn std::error::Error>> {
        // Apply term structure optimizations before normalization
        let optimized_term = if self.config.enable_hash_consing {
            super::hash_cons_deep(term.clone())
        } else {
            std::rc::Rc::new(term.clone())
        };

        // Use standard normalization on optimized term
        standard_normalize(&optimized_term).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }

    fn normalize_parallel_batch(&mut self, terms: &[Term]) -> Vec<Result<Term, Box<dyn std::error::Error>>> {
        // For now, fall back to sequential since parallel infrastructure is complex
        terms.iter().map(|term| self.normalize(term)).collect()
    }

    fn convertible_with_hash_consing(&mut self, term1: &Term, term2: &Term) -> bool {
        // Quick pointer equality check if terms are hash-consed
        if std::ptr::eq(term1, term2) {
            return true;
        }

        // Fall back to standard convertibility
        crate::eval::convertible(term1, term2)
    }
}

/// Convenience functions for one-off operations

/// Normalize a term with default optimizations
pub fn normalize_optimized(term: &Term) -> Result<Term, Box<dyn std::error::Error>> {
    let mut ttt = OptimizedTTT::with_defaults();
    ttt.normalize(term)
}

/// Check convertibility with default optimizations
pub fn convertible_optimized(term1: &Term, term2: &Term) -> bool {
    let mut ttt = OptimizedTTT::with_defaults();
    ttt.convertible(term1, term2)
}

/// Apply substitution with default optimizations
pub fn substitute_optimized(term: &Term, substitution: &crate::core::subst::Substitution) -> Term {
    let mut ttt = OptimizedTTT::with_defaults();
    ttt.substitute(term, substitution)
}

/// Performance benchmarking utilities
pub mod benchmark {
    use super::*;
    use std::time::Instant;

    /// Benchmark normalization performance
    pub fn benchmark_normalize(term: &Term, iterations: usize) -> BenchmarkResult {
        let mut total_time = 0u64;
        let mut successes = 0;

        for _ in 0..iterations {
            let start = Instant::now();
            match standard_normalize(term) {
                Ok(_) => {
                    total_time += start.elapsed().as_nanos() as u64;
                    successes += 1;
                },
                Err(_) => {},
            }
        }

        BenchmarkResult {
            iterations: successes,
            total_time_ns: total_time,
            avg_time_ns: if successes > 0 { total_time / successes as u64 } else { 0 },
            success_rate: successes as f64 / iterations as f64,
        }
    }

    /// Benchmark optimized vs standard normalization
    pub fn compare_normalization(term: &Term, iterations: usize) -> ComparisonResult {
        let standard = benchmark_normalize(term, iterations);

        let mut optimized_time = 0u64;
        let mut optimized_successes = 0;

        for _ in 0..iterations {
            let start = Instant::now();
            match normalize_optimized(term) {
                Ok(_) => {
                    optimized_time += start.elapsed().as_nanos() as u64;
                    optimized_successes += 1;
                },
                Err(_) => {},
            }
        }

        let optimized = BenchmarkResult {
            iterations: optimized_successes,
            total_time_ns: optimized_time,
            avg_time_ns: if optimized_successes > 0 { optimized_time / optimized_successes as u64 } else { 0 },
            success_rate: optimized_successes as f64 / iterations as f64,
        };

        let speedup = if optimized.avg_time_ns > 0 {
            standard.avg_time_ns as f64 / optimized.avg_time_ns as f64
        } else {
            0.0
        };

        ComparisonResult {
            standard,
            optimized,
            speedup,
        }
    }

    #[derive(Debug, Clone)]
    pub struct BenchmarkResult {
        pub iterations: usize,
        pub total_time_ns: u64,
        pub avg_time_ns: u64,
        pub success_rate: f64,
    }

    #[derive(Debug, Clone)]
    pub struct ComparisonResult {
        pub standard: BenchmarkResult,
        pub optimized: BenchmarkResult,
        pub speedup: f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Term;

    #[test]
    fn test_optimized_ttt_creation() {
        let ttt = OptimizedTTT::with_defaults();
        assert!(ttt.config().enable_fusion);
        assert!(ttt.config().enable_hash_consing);
    }

    #[test]
    fn test_normalize_optimized() {
        let term = Term::universe(0);
        let result = normalize_optimized(&term);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convertible_optimized() {
        let term1 = Term::universe(0);
        let term2 = Term::universe(0);
        assert!(convertible_optimized(&term1, &term2));

        let term3 = Term::universe(1);
        assert!(!convertible_optimized(&term1, &term3));
    }

    #[test]
    fn test_substitute_optimized() {
        let term = Term::var(0);
        let replacement = Term::universe(0);
        let subst = crate::core::subst::Substitution::single(0, replacement.clone());

        let result = substitute_optimized(&term, &subst);
        assert_eq!(result, replacement);
    }

    #[test]
    fn test_metrics_tracking() {
        let mut ttt = OptimizedTTT::with_defaults();
        let term = Term::universe(0);

        let _ = ttt.normalize(&term);
        assert!(ttt.metrics().normalization_time_ns > 0);

        ttt.reset_metrics();
        assert_eq!(ttt.metrics().normalization_time_ns, 0);
    }

    #[test]
    fn test_benchmark_utilities() {
        let term = Term::universe(0);
        let result = benchmark::benchmark_normalize(&term, 10);

        assert_eq!(result.iterations, 10);
        assert!(result.total_time_ns > 0);
        assert!(result.avg_time_ns > 0);
        assert_eq!(result.success_rate, 1.0);
    }
}