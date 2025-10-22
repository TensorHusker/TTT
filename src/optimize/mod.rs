//! Performance optimization infrastructure
//!
//! This module provides advanced optimization techniques for the TTT type checker,
//! focusing on algorithmic improvements, memory optimization, and parallel execution.

pub mod fusion;
pub mod memory;
pub mod parallel;
pub mod debruijn;
pub mod api;

pub use fusion::*;
pub use memory::*;
pub use parallel::*;
pub use debruijn::*;
pub use api::{OptimizedTTT, normalize_optimized, convertible_optimized, substitute_optimized};

/// Performance optimization configuration
#[derive(Clone, Debug)]
pub struct OptimizationConfig {
    /// Enable substitution fusion optimizations
    pub enable_fusion: bool,
    /// Enable hash-consing for structural sharing
    pub enable_hash_consing: bool,
    /// Enable parallel normalization
    pub enable_parallel: bool,
    /// Arena allocation block size in bytes
    pub arena_block_size: usize,
    /// Hash table initial capacity for hash-consing
    pub hash_table_capacity: usize,
    /// Number of worker threads for parallel execution
    pub worker_threads: usize,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        OptimizationConfig {
            enable_fusion: true,
            enable_hash_consing: true,
            enable_parallel: true,
            arena_block_size: 4096, // 4KB blocks
            hash_table_capacity: 1024,
            worker_threads: num_cpus::get(),
        }
    }
}

impl OptimizationConfig {
    /// Create configuration optimized for throughput
    pub fn throughput() -> Self {
        OptimizationConfig {
            enable_fusion: true,
            enable_hash_consing: true,
            enable_parallel: true,
            arena_block_size: 8192, // Larger blocks
            hash_table_capacity: 2048, // Larger hash tables
            worker_threads: num_cpus::get(),
        }
    }

    /// Create configuration optimized for low memory usage
    pub fn low_memory() -> Self {
        OptimizationConfig {
            enable_fusion: true,
            enable_hash_consing: false, // Disable hash-consing to save memory
            enable_parallel: false, // Single-threaded to reduce memory overhead
            arena_block_size: 1024, // Smaller blocks
            hash_table_capacity: 256,
            worker_threads: 1,
        }
    }

    /// Create configuration optimized for latency
    pub fn low_latency() -> Self {
        OptimizationConfig {
            enable_fusion: true,
            enable_hash_consing: true,
            enable_parallel: false, // Avoid thread synchronization overhead
            arena_block_size: 2048,
            hash_table_capacity: 512,
            worker_threads: 1,
        }
    }
}

/// Performance metrics collected during optimization
#[derive(Clone, Debug, Default)]
pub struct PerformanceMetrics {
    /// Number of terms allocated
    pub terms_allocated: u64,
    /// Number of hash-cons hits
    pub hash_cons_hits: u64,
    /// Number of hash-cons misses
    pub hash_cons_misses: u64,
    /// Number of substitution fusions applied
    pub fusion_count: u64,
    /// Total bytes allocated
    pub bytes_allocated: u64,
    /// Time spent in normalization (nanoseconds)
    pub normalization_time_ns: u64,
    /// Time spent in conversion checking (nanoseconds)
    pub conversion_time_ns: u64,
    /// Time spent in substitution (nanoseconds)
    pub substitution_time_ns: u64,
}

impl PerformanceMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Default::default()
    }

    /// Get hash-consing hit rate
    pub fn hash_cons_hit_rate(&self) -> f64 {
        let total = self.hash_cons_hits + self.hash_cons_misses;
        if total == 0 {
            0.0
        } else {
            self.hash_cons_hits as f64 / total as f64
        }
    }

    /// Get average term size in bytes
    pub fn avg_term_size(&self) -> f64 {
        if self.terms_allocated == 0 {
            0.0
        } else {
            self.bytes_allocated as f64 / self.terms_allocated as f64
        }
    }

    /// Get total computation time in nanoseconds
    pub fn total_time_ns(&self) -> u64 {
        self.normalization_time_ns + self.conversion_time_ns + self.substitution_time_ns
    }

    /// Merge metrics from another instance
    pub fn merge(&mut self, other: &PerformanceMetrics) {
        self.terms_allocated += other.terms_allocated;
        self.hash_cons_hits += other.hash_cons_hits;
        self.hash_cons_misses += other.hash_cons_misses;
        self.fusion_count += other.fusion_count;
        self.bytes_allocated += other.bytes_allocated;
        self.normalization_time_ns += other.normalization_time_ns;
        self.conversion_time_ns += other.conversion_time_ns;
        self.substitution_time_ns += other.substitution_time_ns;
    }

    /// Reset all metrics to zero
    pub fn reset(&mut self) {
        *self = Default::default();
    }
}

/// Global optimization context
static mut GLOBAL_METRICS: Option<PerformanceMetrics> = None;
static mut GLOBAL_CONFIG: Option<OptimizationConfig> = None;

/// Initialize global optimization context
pub fn init_optimization(config: OptimizationConfig) {
    unsafe {
        GLOBAL_CONFIG = Some(config);
        GLOBAL_METRICS = Some(PerformanceMetrics::new());
    }
}

/// Get current optimization configuration
pub fn get_config() -> OptimizationConfig {
    unsafe {
        GLOBAL_CONFIG.clone().unwrap_or_default()
    }
}

/// Get current performance metrics
pub fn get_metrics() -> PerformanceMetrics {
    unsafe {
        GLOBAL_METRICS.clone().unwrap_or_default()
    }
}

/// Record performance metric
pub fn record_metric<F>(metric_fn: F)
where
    F: FnOnce(&mut PerformanceMetrics),
{
    unsafe {
        if let Some(ref mut metrics) = GLOBAL_METRICS {
            metric_fn(metrics);
        }
    }
}

/// Time a computation and record the result
pub fn time_operation<T, F>(operation: F, record_time: fn(&mut PerformanceMetrics, u64)) -> T
where
    F: FnOnce() -> T,
{
    let start = std::time::Instant::now();
    let result = operation();
    let elapsed = start.elapsed().as_nanos() as u64;

    record_metric(|metrics| record_time(metrics, elapsed));
    result
}