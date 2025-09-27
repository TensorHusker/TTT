#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

//! # TTT - Tiny Type Theory
//!
//! A categorical kernel of dependent types implementing Π/Σ/≡ types
//! via zero-cost abstractions for type-directed compression and verification.

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;
use alloc::sync::Arc;

pub mod arena;
pub mod check;
pub mod compress;
pub mod nbe;
pub mod term;
pub mod types;

pub use arena::Arena;
pub use check::{TypeError, TypeChecker};
pub use compress::TypeCompressible;
pub use nbe::Normalizer;
pub use term::{Term, Level};
pub use types::{Type, Universe, TypeEnv};

/// Core result type for type checking operations
pub type CheckResult<T> = Result<Arc<T>, TypeError>;

#[cfg(test)]
mod tests;