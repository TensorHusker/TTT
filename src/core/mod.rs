//! Core module for TTT dependent type theory
//!
//! This module contains the fundamental data structures and operations
//! for representing and manipulating dependent type theory terms.

pub mod term;
pub mod value;
pub mod subst;
pub mod properties;

pub use term::{Term, Level};
pub use value::{Value, Closure, Environment, Neutral};
pub use subst::{Substitution, apply_substitution, shift_term, substitute_top};
pub use properties::{verify_substitution_lemma, verify_type_preservation_substitution, verify_all_properties};

/// Type aliases for clarity and documentation
pub type DeBruijnIndex = usize;
pub type DeBruijnLevel = usize;
pub type BindingDepth = usize;