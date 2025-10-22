//! Bidirectional type checking with constraint solving
//!
//! This module implements a bidirectional type checker for dependent types
//! with support for metavariables and constraint solving.

pub mod context;
pub mod constraints;
pub mod checker;
pub mod inference;

pub use context::{Context, ContextEntry, LocalContext};
pub use checker::{TypeChecker, check_term, infer_type};
pub use constraints::{Constraint, ConstraintSolver, UnificationError};
pub use inference::{MetavarGenerator, fresh_meta};

use crate::core::{Term, Value, Level};

/// Result type for type checking operations
pub type CheckResult<T> = Result<T, CheckError>;

/// Type checking errors
#[derive(Debug, Clone, PartialEq)]
pub enum CheckError {
    /// Type mismatch between expected and actual types
    TypeMismatch {
        expected: Value,
        actual: Value,
        term: Term,
    },
    /// Variable not found in context
    UnboundVariable(usize),
    /// Universe level mismatch
    UniverseMismatch {
        expected_level: Level,
        actual_level: Level,
    },
    /// Cannot infer type for term
    CannotInfer(Term),
    /// Function expected but got different type
    NotAFunction {
        function_type: Value,
        argument: Term,
    },
    /// Constraint solving failed
    ConstraintError(UnificationError),
    /// Occurs check failure in unification
    OccursCheck {
        metavar: usize,
        term: Term,
    },
    /// Invalid universe levels
    InvalidLevel(String),
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::TypeMismatch { expected, actual, term } => {
                write!(
                    f,
                    "Type mismatch: expected '{}', got '{}' for term '{}'",
                    expected, actual, term
                )
            }
            CheckError::UnboundVariable(index) => {
                write!(f, "Unbound variable: #{}", index)
            }
            CheckError::UniverseMismatch { expected_level, actual_level } => {
                write!(
                    f,
                    "Universe level mismatch: expected Type{}, got Type{}",
                    expected_level, actual_level
                )
            }
            CheckError::CannotInfer(term) => {
                write!(f, "Cannot infer type for term: {}", term)
            }
            CheckError::NotAFunction { function_type, argument } => {
                write!(
                    f,
                    "Expected function type, got '{}' when applying to '{}'",
                    function_type, argument
                )
            }
            CheckError::ConstraintError(err) => {
                write!(f, "Constraint solving error: {}", err)
            }
            CheckError::OccursCheck { metavar, term } => {
                write!(f, "Occurs check: metavar ?{} occurs in {}", metavar, term)
            }
            CheckError::InvalidLevel(msg) => {
                write!(f, "Invalid universe level: {}", msg)
            }
        }
    }
}

impl std::error::Error for CheckError {}

impl From<UnificationError> for CheckError {
    fn from(err: UnificationError) -> Self {
        CheckError::ConstraintError(err)
    }
}

/// Main type checking entry point
///
/// Checks that a term has the expected type in the given context.
pub fn check(term: &Term, expected_type: &Value, context: &Context) -> CheckResult<()> {
    let mut checker = TypeChecker::new();
    checker.check(term, expected_type, context)
}

/// Main type inference entry point
///
/// Infers the type of a term in the given context.
pub fn infer(term: &Term, context: &Context) -> CheckResult<Value> {
    let mut checker = TypeChecker::new();
    checker.infer(term, context)
}

/// Check that a type is well-formed
///
/// Verifies that a type is valid and computes its universe level.
pub fn check_type(typ: &Term, context: &Context) -> CheckResult<Level> {
    let mut checker = TypeChecker::new();
    let inferred_type = checker.infer(typ, context)?;

    match inferred_type {
        Value::Universe(level) => Ok(level),
        _ => Err(CheckError::InvalidLevel(
            format!("Expected universe, got {}", inferred_type)
        )),
    }
}

/// Convert between De Bruijn indices and levels
///
/// Used during type checking to maintain proper variable binding.
pub fn index_to_level(index: usize, context_length: usize) -> Option<usize> {
    if index < context_length {
        Some(context_length - 1 - index)
    } else {
        None
    }
}

/// Convert De Bruijn level to index
pub fn level_to_index(level: usize, context_length: usize) -> Option<usize> {
    if level < context_length {
        Some(context_length - 1 - level)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Environment;

    #[test]
    fn test_check_universe() {
        let context = Context::empty();
        let term = Term::universe(0);
        let expected = Value::universe(Level(1));

        assert!(check(&term, &expected, &context).is_ok());
    }

    #[test]
    fn test_infer_universe() {
        let context = Context::empty();
        let term = Term::universe(0);

        let result = infer(&term, &context).unwrap();
        assert_eq!(result, Value::universe(Level(1)));
    }

    #[test]
    fn test_check_variable() {
        let mut context = Context::empty();
        let var_type = Value::universe(Level::TYPE);
        context = context.extend("x".to_string(), var_type.clone());

        let term = Term::var(0);
        assert!(check(&term, &var_type, &context).is_ok());
    }

    #[test]
    fn test_index_level_conversion() {
        assert_eq!(index_to_level(0, 3), Some(2));
        assert_eq!(index_to_level(1, 3), Some(1));
        assert_eq!(index_to_level(2, 3), Some(0));
        assert_eq!(index_to_level(3, 3), None);

        assert_eq!(level_to_index(0, 3), Some(2));
        assert_eq!(level_to_index(1, 3), Some(1));
        assert_eq!(level_to_index(2, 3), Some(0));
        assert_eq!(level_to_index(3, 3), None);
    }

    #[test]
    fn test_check_type() {
        let context = Context::empty();
        let type_term = Term::universe(0);

        let level = check_type(&type_term, &context).unwrap();
        assert_eq!(level, Level(1));
    }
}