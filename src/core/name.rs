//! Name representation for TTT
//!
//! This module provides support for named variables as an alternative
//! to De Bruijn indices, which is essential for interoperability with
//! systems like Lean that use named variables.

use std::fmt;
use std::hash::{Hash, Hasher};

/// A name in TTT
///
/// Supports both user-provided names and generated names for
/// anonymous bindings. Names are interned for efficient comparison.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name {
    /// The string representation of the name
    name: String,
    /// Optional unique identifier for generated names
    id: Option<u64>,
}

impl Name {
    /// Create a new user-provided name
    pub fn user(name: impl Into<String>) -> Self {
        Name {
            name: name.into(),
            id: None,
        }
    }

    /// Create a fresh generated name with a hint
    pub fn fresh(hint: impl Into<String>) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        Name {
            name: hint.into(),
            id: Some(id),
        }
    }

    /// Create an anonymous name
    pub fn anonymous() -> Self {
        Self::fresh("_")
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Check if this is a generated name
    pub fn is_generated(&self) -> bool {
        self.id.is_some()
    }

    /// Check if this is an anonymous name
    pub fn is_anonymous(&self) -> bool {
        self.name == "_"
    }

    /// Get a unique identifier for this name
    pub fn unique_id(&self) -> String {
        match self.id {
            Some(id) => format!("{}#{}", self.name, id),
            None => self.name.clone(),
        }
    }
}

impl From<&str> for Name {
    fn from(name: &str) -> Self {
        Name::user(name)
    }
}

impl From<String> for Name {
    fn from(name: String) -> Self {
        Name::user(name)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(id) = self.id {
            write!(f, "{}#{}", self.name, id)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

/// A context mapping names to De Bruijn indices
///
/// This is used for converting between named and De Bruijn representations.
#[derive(Clone, Debug, Default)]
pub struct NameContext {
    /// Stack of bound names (innermost first)
    names: Vec<Name>,
}

impl NameContext {
    /// Create a new empty context
    pub fn new() -> Self {
        NameContext {
            names: Vec::new(),
        }
    }

    /// Bind a new name, returning the updated context
    pub fn bind(&self, name: Name) -> Self {
        let mut new_names = self.names.clone();
        new_names.push(name);
        NameContext { names: new_names }
    }

    /// Look up a name to get its De Bruijn index
    pub fn lookup(&self, name: &Name) -> Option<usize> {
        // Search from the end (most recent binding)
        for (i, bound_name) in self.names.iter().rev().enumerate() {
            if bound_name == name {
                return Some(i);
            }
        }
        None
    }

    /// Get the name at a given De Bruijn index
    pub fn name_at(&self, index: usize) -> Option<&Name> {
        if index < self.names.len() {
            let pos = self.names.len() - 1 - index;
            Some(&self.names[pos])
        } else {
            None
        }
    }

    /// Get the current binding depth
    pub fn depth(&self) -> usize {
        self.names.len()
    }

    /// Check if context is empty
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Generate a fresh name that doesn't conflict with existing bindings
    pub fn fresh_name(&self, hint: &str) -> Name {
        let candidate = Name::user(hint);

        // Check if the name already exists
        if self.lookup(&candidate).is_none() {
            candidate
        } else {
            // Generate a fresh name with a suffix
            let mut counter = 1;
            loop {
                let new_name = Name::user(format!("{}_{}", hint, counter));
                if self.lookup(&new_name).is_none() {
                    return new_name;
                }
                counter += 1;
            }
        }
    }

    /// Get all bound names
    pub fn names(&self) -> &[Name] {
        &self.names
    }
}

impl fmt::Display for NameContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, name) in self.names.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", name)?;
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_creation() {
        let user_name = Name::user("x");
        assert!(!user_name.is_generated());
        assert!(!user_name.is_anonymous());
        assert_eq!(user_name.as_str(), "x");

        let fresh_name = Name::fresh("y");
        assert!(fresh_name.is_generated());
        assert!(!fresh_name.is_anonymous());
        assert_eq!(fresh_name.as_str(), "y");

        let anon_name = Name::anonymous();
        assert!(anon_name.is_generated());
        assert!(anon_name.is_anonymous());
    }

    #[test]
    fn test_name_context() {
        let ctx = NameContext::new();
        assert!(ctx.is_empty());
        assert_eq!(ctx.depth(), 0);

        let x = Name::user("x");
        let ctx1 = ctx.bind(x.clone());
        assert_eq!(ctx1.depth(), 1);
        assert_eq!(ctx1.lookup(&x), Some(0));

        let y = Name::user("y");
        let ctx2 = ctx1.bind(y.clone());
        assert_eq!(ctx2.depth(), 2);
        assert_eq!(ctx2.lookup(&y), Some(0)); // Most recent
        assert_eq!(ctx2.lookup(&x), Some(1)); // Previous

        assert_eq!(ctx2.name_at(0), Some(&y));
        assert_eq!(ctx2.name_at(1), Some(&x));
    }

    #[test]
    fn test_fresh_name_generation() {
        let ctx = NameContext::new()
            .bind(Name::user("x"))
            .bind(Name::user("x_1"));

        let fresh = ctx.fresh_name("x");
        assert_eq!(fresh.as_str(), "x_2");
        assert!(ctx.lookup(&fresh).is_none());
    }
}