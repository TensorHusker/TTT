//! Error types for TTT-Lean integration

use thiserror::Error;
use std::fmt;

pub type Result<T> = std::result::Result<T, LeanError>;

#[derive(Debug, Error)]
pub enum LeanError {
    #[error("Translation failed: {0}")]
    Translation(String),

    #[error("Universe level {0} not supported")]
    UniverseLevel(u32),

    #[error("Variable not found: {0}")]
    UnboundVariable(String),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: String,
        actual: String,
    },

    #[error("Unsupported term: {0}")]
    UnsupportedTerm(String),

    #[error("Name resolution failed: {0}")]
    NameResolution(String),

    #[error("Context error: {0}")]
    Context(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl LeanError {
    /// Create a translation error
    pub fn translation(msg: impl Into<String>) -> Self {
        LeanError::Translation(msg.into())
    }

    /// Create an unsupported term error
    pub fn unsupported_term(term: impl fmt::Display) -> Self {
        LeanError::UnsupportedTerm(format!("{}", term))
    }

    /// Create a name resolution error
    pub fn name_resolution(msg: impl Into<String>) -> Self {
        LeanError::NameResolution(msg.into())
    }

    /// Create a context error
    pub fn context(msg: impl Into<String>) -> Self {
        LeanError::Context(msg.into())
    }

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        LeanError::Internal(msg.into())
    }
}