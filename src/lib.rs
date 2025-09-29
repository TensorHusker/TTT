//! TTT - Tiny Type Theory
//!
//! A high-performance dependent type theory kernel with normalization by evaluation,
//! bidirectional type checking, and ARC pattern synthesis capabilities.

pub mod core;
pub mod eval;
pub mod optimize;
pub mod typeck;

// Re-export main types and functions
pub use core::{Term, Level, Value, Environment};
pub use eval::{normalize, convertible, apply_value};
pub use typeck::{Context, TypeChecker, check_term, infer_type};
pub use optimize::{OptimizationConfig, PerformanceMetrics, OptimizedTTT, normalize_optimized, convertible_optimized};
