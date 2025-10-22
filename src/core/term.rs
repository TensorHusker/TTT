//! Core term representation for dependent type theory
//!
//! This module defines the abstract syntax tree for TTT terms using
//! De Bruijn indices for variable representation. The design prioritizes
//! structural sharing through Rc<T> and supports parallel substitution.

use std::sync::Arc;
use std::fmt;
use serde::{Serialize, Deserialize};

/// Universe levels for type stratification
///
/// Prevents Russell's paradox by stratifying types into a hierarchy:
/// Type₀ : Type₁ : Type₂ : ...
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Level(pub u32);

impl Level {
    /// Base universe level Type₀
    pub const TYPE: Level = Level(0);

    /// Maximum level that can be represented
    pub const MAX: Level = Level(u32::MAX);

    /// Successor level
    #[inline]
    pub fn succ(&self) -> Level {
        Level(self.0.saturating_add(1))
    }

    /// Maximum of two levels
    #[inline]
    pub fn max(&self, other: &Level) -> Level {
        Level(self.0.max(other.0))
    }

    /// Check if this level is zero
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Get the raw level value
    #[inline]
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Core term structure for dependent type theory
///
/// Uses De Bruijn indices for variables where 0 refers to the
/// most recently bound variable. All composite terms use Arc<T>
/// for structural sharing and efficient cloning.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Term {
    /// Variable with De Bruijn index
    ///
    /// Index 0 refers to the most recent binding, 1 to the next outer, etc.
    /// Example: λx.λy.x has body λy.1 (x is at index 1 from inner λy)
    Var(usize),

    /// Type universe at given level
    ///
    /// Type₀ : Type₁ : Type₂ : ... forms the universe hierarchy
    /// Prevents Russell's paradox through stratification
    Universe(Level),

    /// Dependent function type: Π(x:A).B
    ///
    /// First component is domain A, second is codomain B which may
    /// depend on x (represented as De Bruijn index 0 in B)
    Pi(
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>,
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>
    ),

    /// Lambda abstraction: λx.e
    ///
    /// Binds a variable in the body term. Variable is accessed
    /// via De Bruijn index 0 in the body
    Lambda(
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>
    ),

    /// Function application: f x
    ///
    /// Applies function f to argument x
    App(
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>,
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>
    ),

    /// Let binding: let x = e₁ in e₂
    ///
    /// Syntactic sugar for (λx.e₂) e₁ but enables better optimization
    Let(
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>,
        #[serde(serialize_with = "arc_serde::serialize", deserialize_with = "arc_serde::deserialize")]
        Arc<Term>
    ),

    /// Metavariable for type inference
    ///
    /// Represents an unknown term to be solved during type checking
    /// The usize is a unique identifier for the metavariable
    Meta(usize),
}

mod arc_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S, T>(arc: &Arc<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        arc.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Arc<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        T::deserialize(deserializer).map(Arc::new)
    }
}

/// Constructor functions and term analysis
impl Term {
    /// Create a variable with De Bruijn index
    #[inline]
    pub fn var(index: usize) -> Self {
        Term::Var(index)
    }

    /// Create a universe at given level
    #[inline]
    pub fn universe(level: u32) -> Self {
        Term::Universe(Level(level))
    }

    /// Create a dependent function type Π(x:A).B
    #[inline]
    pub fn pi(domain: Term, codomain: Term) -> Self {
        Term::Pi(Arc::new(domain), Arc::new(codomain))
    }

    /// Create a lambda abstraction λx.e
    #[inline]
    pub fn lambda(body: Term) -> Self {
        Term::Lambda(Arc::new(body))
    }

    /// Create a function application f x
    #[inline]
    pub fn app(function: Term, argument: Term) -> Self {
        Term::App(Arc::new(function), Arc::new(argument))
    }

    /// Create a let binding let x = e₁ in e₂
    #[inline]
    pub fn let_in(binding: Term, body: Term) -> Self {
        Term::Let(Arc::new(binding), Arc::new(body))
    }

    /// Create a metavariable
    #[inline]
    pub fn meta(id: usize) -> Self {
        Term::Meta(id)
    }

    /// Create Type₀
    #[inline]
    pub fn type_0() -> Self {
        Term::Universe(Level::TYPE)
    }

    /// Create Type₁
    #[inline]
    pub fn type_1() -> Self {
        Term::Universe(Level::TYPE.succ())
    }

    /// Check if term is a variable
    #[inline]
    pub fn is_var(&self) -> bool {
        matches!(self, Term::Var(_))
    }

    /// Check if term is a universe
    #[inline]
    pub fn is_universe(&self) -> bool {
        matches!(self, Term::Universe(_))
    }

    /// Check if term is a function type
    #[inline]
    pub fn is_pi(&self) -> bool {
        matches!(self, Term::Pi(_, _))
    }

    /// Check if term is a lambda
    #[inline]
    pub fn is_lambda(&self) -> bool {
        matches!(self, Term::Lambda(_))
    }

    /// Check if term is an application
    #[inline]
    pub fn is_app(&self) -> bool {
        matches!(self, Term::App(_, _))
    }

    /// Check if term is a metavariable
    #[inline]
    pub fn is_meta(&self) -> bool {
        matches!(self, Term::Meta(_))
    }

    /// Get the maximum De Bruijn index in the term
    pub fn max_index(&self) -> Option<usize> {
        match self {
            Term::Var(i) => Some(*i),
            Term::Universe(_) => None,
            Term::Pi(dom, cod) => {
                let dom_max = dom.max_index();
                let cod_max = cod.max_index();
                match (dom_max, cod_max) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            },
            Term::Lambda(body) => body.max_index(),
            Term::App(fun, arg) => {
                let fun_max = fun.max_index();
                let arg_max = arg.max_index();
                match (fun_max, arg_max) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            },
            Term::Let(binding, body) => {
                let bind_max = binding.max_index();
                let body_max = body.max_index();
                match (bind_max, body_max) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            },
            Term::Meta(_) => None,
        }
    }

    /// Check if term is closed (no free variables)
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.max_index().is_none()
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_names(f, &[])
    }
}

impl Term {
    /// Format term with variable names for debugging
    pub fn fmt_with_names(&self, f: &mut fmt::Formatter<'_>, names: &[&str]) -> fmt::Result {
        match self {
            Term::Var(i) => {
                if *i < names.len() {
                    write!(f, "{}", names[names.len() - 1 - i])
                } else {
                    write!(f, "#{}", i)
                }
            },
            Term::Universe(level) => write!(f, "Type{}", level),
            Term::Pi(dom, cod) => {
                write!(f, "(Π _ : ")?;
                dom.fmt_with_names(f, names)?;
                write!(f, ". ")?;
                let new_names = {
                    let mut new_names = names.to_vec();
                    new_names.push("x");
                    new_names
                };
                cod.fmt_with_names(f, &new_names)?;
                write!(f, ")")
            },
            Term::Lambda(body) => {
                write!(f, "(λ ")?;
                let new_names = {
                    let mut new_names = names.to_vec();
                    new_names.push("x");
                    new_names
                };
                body.fmt_with_names(f, &new_names)?;
                write!(f, ")")
            },
            Term::App(fun, arg) => {
                write!(f, "(")?;
                fun.fmt_with_names(f, names)?;
                write!(f, " ")?;
                arg.fmt_with_names(f, names)?;
                write!(f, ")")
            },
            Term::Let(binding, body) => {
                write!(f, "(let ")?;
                binding.fmt_with_names(f, names)?;
                write!(f, " in ")?;
                let new_names = {
                    let mut new_names = names.to_vec();
                    new_names.push("x");
                    new_names
                };
                body.fmt_with_names(f, &new_names)?;
                write!(f, ")")
            },
            Term::Meta(id) => write!(f, "?{}", id),
        }
    }
}