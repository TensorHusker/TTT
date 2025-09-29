//! Semantic domain for normalization by evaluation
//!
//! Values represent the semantic interpretation of terms during evaluation.
//! They enable efficient normalization and convertibility checking while
//! maintaining the mathematical structure of dependent type theory.

use std::rc::Rc;
use std::fmt;
use crate::core::{Term, Level};

/// Semantic values for normalization by evaluation
///
/// Values represent terms in weak-head normal form and support
/// efficient composition during normalization. Closures capture
/// environments for lazy evaluation.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Variable that couldn't be reduced further
    ///
    /// Either a free variable or a bound variable that hasn't been
    /// substituted yet. Contains the De Bruijn level (not index).
    Var(usize),

    /// Universe value at given level
    Universe(Level),

    /// Dependent function type value
    ///
    /// Domain is already evaluated, codomain is a closure that
    /// will be evaluated when applied to an argument.
    Pi(Rc<Value>, Closure),

    /// Lambda function value
    ///
    /// Body is stored as a closure to enable lazy evaluation
    /// and proper capture of the environment.
    Lambda(Closure),

    /// Neutral term - a variable applied to arguments
    ///
    /// Represents terms that are stuck on a variable but have
    /// been partially applied. Used for η-expansion and quotation.
    Neutral(Neutral),
}

/// Closure for lazy evaluation
///
/// Captures an environment and a term that can be evaluated
/// later when needed. Enables efficient substitution and
/// prevents unnecessary recomputation.
#[derive(Clone, Debug, PartialEq)]
pub struct Closure {
    /// Environment mapping De Bruijn indices to values
    pub env: Environment,
    /// Term to be evaluated in the environment
    pub term: Rc<Term>,
}

/// Environment for variable lookup
///
/// Maps De Bruijn indices to their corresponding values.
/// Uses a vector for efficient access and structural sharing.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Environment {
    /// Values for each De Bruijn level
    /// Index 0 corresponds to level 0 (outermost binding)
    values: Vec<Rc<Value>>,
}

/// Neutral terms - variables with spines
///
/// Represents terms that are stuck on a variable but may
/// have arguments applied to them. Used for η-expansion
/// and maintaining canonical forms during normalization.
#[derive(Clone, Debug, PartialEq)]
pub struct Neutral {
    /// The head variable (De Bruijn level)
    pub head: usize,
    /// Spine of applications
    pub spine: Vec<Rc<Value>>,
}

impl Value {
    /// Create a variable value at given level
    #[inline]
    pub fn var(level: usize) -> Self {
        Value::Var(level)
    }

    /// Create a universe value
    #[inline]
    pub fn universe(level: Level) -> Self {
        Value::Universe(level)
    }

    /// Create a Pi type value
    #[inline]
    pub fn pi(domain: Value, codomain: Closure) -> Self {
        Value::Pi(Rc::new(domain), codomain)
    }

    /// Create a lambda value
    #[inline]
    pub fn lambda(closure: Closure) -> Self {
        Value::Lambda(closure)
    }

    /// Create a neutral value
    #[inline]
    pub fn neutral(head: usize, spine: Vec<Rc<Value>>) -> Self {
        Value::Neutral(Neutral { head, spine })
    }

    /// Check if value is in weak-head normal form
    #[inline]
    pub fn is_whnf(&self) -> bool {
        match self {
            Value::Var(_) | Value::Universe(_) | Value::Pi(_, _) | Value::Lambda(_) => true,
            Value::Neutral(_) => true,
        }
    }

    /// Check if value is a universe
    #[inline]
    pub fn is_universe(&self) -> bool {
        matches!(self, Value::Universe(_))
    }

    /// Check if value is a Pi type
    #[inline]
    pub fn is_pi(&self) -> bool {
        matches!(self, Value::Pi(_, _))
    }

    /// Check if value is a lambda
    #[inline]
    pub fn is_lambda(&self) -> bool {
        matches!(self, Value::Lambda(_))
    }

    /// Extract universe level if this is a universe
    pub fn as_universe(&self) -> Option<&Level> {
        match self {
            Value::Universe(level) => Some(level),
            _ => None,
        }
    }

    /// Extract Pi type components if this is a Pi type
    pub fn as_pi(&self) -> Option<(&Value, &Closure)> {
        match self {
            Value::Pi(domain, codomain) => Some((domain, codomain)),
            _ => None,
        }
    }

    /// Extract lambda closure if this is a lambda
    pub fn as_lambda(&self) -> Option<&Closure> {
        match self {
            Value::Lambda(closure) => Some(closure),
            _ => None,
        }
    }
}

impl Closure {
    /// Create a new closure
    #[inline]
    pub fn new(env: Environment, term: Term) -> Self {
        Closure {
            env,
            term: Rc::new(term),
        }
    }

    /// Create a closure with empty environment
    #[inline]
    pub fn empty(term: Term) -> Self {
        Closure {
            env: Environment::default(),
            term: Rc::new(term),
        }
    }

    /// Extend closure environment with a new value
    pub fn extend(&self, value: Value) -> Self {
        Closure {
            env: self.env.extend(value),
            term: self.term.clone(),
        }
    }
}

impl Environment {
    /// Create a new empty environment
    #[inline]
    pub fn new() -> Self {
        Environment {
            values: Vec::new(),
        }
    }

    /// Get the length of the environment
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if environment is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Extend environment with a new value
    ///
    /// The new value becomes accessible at index equal to the
    /// current length of the environment.
    pub fn extend(&self, value: Value) -> Self {
        let mut new_values = self.values.clone();
        new_values.push(Rc::new(value));
        Environment { values: new_values }
    }

    /// Lookup a value by De Bruijn index
    ///
    /// Returns None if index is out of bounds. Index 0 refers
    /// to the most recently bound variable.
    pub fn lookup(&self, index: usize) -> Option<&Value> {
        if index < self.values.len() {
            let level = self.values.len() - 1 - index;
            Some(&self.values[level])
        } else {
            None
        }
    }

    /// Convert De Bruijn index to level
    ///
    /// Level is the position from the beginning of the environment,
    /// while index is the position from the end.
    #[inline]
    pub fn index_to_level(&self, index: usize) -> Option<usize> {
        if index < self.values.len() {
            Some(self.values.len() - 1 - index)
        } else {
            None
        }
    }

    /// Get all values as a slice
    #[inline]
    pub fn values(&self) -> &[Rc<Value>] {
        &self.values
    }
}

impl Neutral {
    /// Create a new neutral term
    #[inline]
    pub fn new(head: usize) -> Self {
        Neutral {
            head,
            spine: Vec::new(),
        }
    }

    /// Apply an argument to the neutral term
    pub fn apply(&self, arg: Value) -> Self {
        let mut new_spine = self.spine.clone();
        new_spine.push(Rc::new(arg));
        Neutral {
            head: self.head,
            spine: new_spine,
        }
    }

    /// Get the number of arguments applied
    #[inline]
    pub fn arity(&self) -> usize {
        self.spine.len()
    }

    /// Check if neutral has no arguments
    #[inline]
    pub fn is_variable(&self) -> bool {
        self.spine.is_empty()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Var(level) => write!(f, "#{}", level),
            Value::Universe(level) => write!(f, "Type{}", level),
            Value::Pi(domain, _) => {
                write!(f, "(Π _ : {}. ...)", domain)
            },
            Value::Lambda(_) => write!(f, "(λ ...)"),
            Value::Neutral(neutral) => {
                write!(f, "#{}", neutral.head)?;
                for arg in &neutral.spine {
                    write!(f, " {}", arg)?;
                }
                Ok(())
            },
        }
    }
}

impl fmt::Display for Closure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "⟨{} | {}⟩", self.env, self.term)
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, value) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", value)?;
        }
        write!(f, "]")
    }
}

impl fmt::Display for Neutral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.head)?;
        for arg in &self.spine {
            write!(f, " {}", arg)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_operations() {
        let env = Environment::new();
        assert!(env.is_empty());
        assert_eq!(env.len(), 0);

        let val1 = Value::universe(Level::TYPE);
        let env1 = env.extend(val1.clone());
        assert_eq!(env1.len(), 1);
        assert_eq!(env1.lookup(0), Some(&val1));

        let val2 = Value::var(0);
        let env2 = env1.extend(val2.clone());
        assert_eq!(env2.len(), 2);
        assert_eq!(env2.lookup(0), Some(&val2)); // Most recent
        assert_eq!(env2.lookup(1), Some(&val1)); // Previous
    }

    #[test]
    fn test_neutral_application() {
        let neutral = Neutral::new(0);
        assert!(neutral.is_variable());
        assert_eq!(neutral.arity(), 0);

        let arg = Value::universe(Level::TYPE);
        let applied = neutral.apply(arg.clone());
        assert!(!applied.is_variable());
        assert_eq!(applied.arity(), 1);
    }

    #[test]
    fn test_value_constructors() {
        let var = Value::var(0);
        assert!(!var.is_universe());
        assert!(!var.is_pi());
        assert!(!var.is_lambda());

        let universe = Value::universe(Level::TYPE);
        assert!(universe.is_universe());
        assert_eq!(universe.as_universe(), Some(&Level::TYPE));

        let closure = Closure::empty(Term::var(0));
        let lambda = Value::lambda(closure);
        assert!(lambda.is_lambda());
    }
}