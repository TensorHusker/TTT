//! Normalization by Evaluation (NbE) system
//!
//! This module implements the core evaluation and quotation algorithms
//! that enable efficient normalization and convertibility checking in TTT.

pub mod normalize;
pub mod quotation;
pub mod apply;
pub mod convert;

pub use normalize::{normalize, normalize_in_env, weak_head_normalize, evaluate};
pub use quotation::{quote, quote_neutral};
pub use apply::{apply_value, instantiate_closure};
pub use convert::{convertible_with_type, convertible_structural, definitionally_equal};

/// Compatibility wrapper for the convertible function
pub fn convertible(term1: &crate::core::Term, term2: &crate::core::Term) -> bool {
    // First normalize both terms, then check structural convertibility
    if let (Ok(val1), Ok(val2)) = (
        normalize(term1).and_then(|t| evaluate(&t, &crate::core::Environment::new())),
        normalize(term2).and_then(|t| evaluate(&t, &crate::core::Environment::new()))
    ) {
        convertible_structural(&val1, &val2, 0)
    } else {
        false
    }
}

use crate::core::{Value, Level};

/// Error types for evaluation
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// Variable not found in environment
    UnboundVariable(usize),
    /// Type mismatch during application
    TypeMismatch(String),
    /// Attempted to apply non-function
    NotAFunction(Value),
    /// Universe level overflow
    LevelOverflow,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::UnboundVariable(index) => {
                write!(f, "Unbound variable: #{}", index)
            },
            EvalError::TypeMismatch(msg) => {
                write!(f, "Type mismatch: {}", msg)
            },
            EvalError::NotAFunction(value) => {
                write!(f, "Not a function: {}", value)
            },
            EvalError::LevelOverflow => {
                write!(f, "Universe level overflow")
            },
        }
    }
}

impl std::error::Error for EvalError {}

pub type EvalResult<T> = Result<T, EvalError>;

/// Check if two values are convertible (definitionally equal)
///
/// This is the main convertibility checker used by the type checker.
/// Two terms are convertible if they normalize to the same value.
pub fn convertible_values(val1: &Value, val2: &Value, level: usize) -> bool {
    convertible_aux(val1, val2, level)
}

fn convertible_aux(val1: &Value, val2: &Value, level: usize) -> bool {
    match (val1, val2) {
        (Value::Var(l1), Value::Var(l2)) => l1 == l2,

        (Value::Universe(level1), Value::Universe(level2)) => level1 == level2,

        (Value::Pi(dom1, cod1), Value::Pi(dom2, cod2)) => {
            convertible_aux(dom1, dom2, level) && {
                let var = Value::var(level);
                let cod1_inst = instantiate_closure(cod1, var.clone()).unwrap_or_else(|_| var.clone());
                let cod2_inst = instantiate_closure(cod2, var.clone()).unwrap_or_else(|_| var);
                convertible_aux(&cod1_inst, &cod2_inst, level + 1)
            }
        },

        (Value::Lambda(closure1), Value::Lambda(closure2)) => {
            let var = Value::var(level);
            let body1 = instantiate_closure(closure1, var.clone()).unwrap_or_else(|_| var.clone());
            let body2 = instantiate_closure(closure2, var.clone()).unwrap_or_else(|_| var);
            convertible_aux(&body1, &body2, level + 1)
        },

        (Value::Neutral(n1), Value::Neutral(n2)) => {
            n1.head == n2.head &&
            n1.spine.len() == n2.spine.len() &&
            n1.spine.iter().zip(n2.spine.iter()).all(|(arg1, arg2)| {
                convertible_aux(arg1, arg2, level)
            })
        },

        // η-expansion for functions
        (Value::Lambda(closure), other) | (other, Value::Lambda(closure)) => {
            let var = Value::var(level);
            let body = instantiate_closure(closure, var.clone()).unwrap_or_else(|_| var.clone());
            let other_applied = apply_value(other.clone(), var).unwrap_or_else(|_| other.clone());
            convertible_aux(&body, &other_applied, level + 1)
        },

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Term, Closure, Environment};

    #[test]
    fn test_convertible_variables() {
        let var1 = Value::var(0);
        let var2 = Value::var(0);
        let var3 = Value::var(1);

        assert!(convertible_values(&var1, &var2, 0));
        assert!(!convertible_values(&var1, &var3, 0));
    }

    #[test]
    fn test_convertible_universes() {
        let type0 = Value::universe(Level::TYPE);
        let type1 = Value::universe(Level::TYPE.succ());

        assert!(convertible_values(&type0, &type0, 0));
        assert!(!convertible_values(&type0, &type1, 0));
    }

    #[test]
    fn test_convertible_pi_types() {
        // Π(x:Type₀).Type₀ should be convertible to itself
        let domain = Value::universe(Level::TYPE);
        let codomain = Closure::empty(Term::universe(0));
        let pi1 = Value::pi(domain.clone(), codomain.clone());
        let pi2 = Value::pi(domain, codomain);

        assert!(convertible_values(&pi1, &pi2, 0));
    }
}