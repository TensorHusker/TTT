//! Property-based testing module for TTT verification
//!
//! This module contains comprehensive property-based tests that verify
//! the mathematical correctness of the TTT type theory implementation.

pub mod generators;
pub mod substitution_props;
pub mod type_preservation;
pub mod confluence_tests;
pub mod conversion_props;

// Re-export commonly used items
pub use generators::*;