//! Core term representation using De Bruijn indices and levels

use alloc::sync::Arc;
use core::fmt;

/// De Bruijn level for variable indexing with const generics
pub type Level = u32;

/// Core term representation implementing the categorical structure
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// Variable with De Bruijn level
    Var(Level),
    
    /// Π-types (dependent function types) - (x: A) -> B
    Pi {
        name: Option<Arc<str>>,
        domain: Arc<Term>,
        codomain: Arc<Term>,
    },
    
    /// Lambda abstraction - λx. body
    Lambda {
        name: Option<Arc<str>>,
        body: Arc<Term>,
    },
    
    /// Function application - f(x)
    App {
        func: Arc<Term>,
        arg: Arc<Term>,
    },
    
    /// Σ-types (dependent pair types) - (x: A) × B
    Sigma {
        name: Option<Arc<str>>,
        first: Arc<Term>,
        second: Arc<Term>,
    },
    
    /// Pair construction - (a, b)
    Pair {
        first: Arc<Term>,
        second: Arc<Term>,
    },
    
    /// First projection - π₁(p)
    Fst(Arc<Term>),
    
    /// Second projection - π₂(p)
    Snd(Arc<Term>),
    
    /// Identity/Equality types - a ≡ b
    Id {
        ty: Arc<Term>,
        left: Arc<Term>,
        right: Arc<Term>,
    },
    
    /// Reflexivity proof - refl
    Refl(Arc<Term>),
    
    /// Universe/Type at level
    Universe(Level),
}

impl Term {
    /// Create a variable term
    pub fn var(level: Level) -> Arc<Self> {
        Arc::new(Term::Var(level))
    }
    
    /// Create a Pi type
    pub fn pi(name: Option<Arc<str>>, domain: Arc<Term>, codomain: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Pi { name, domain, codomain })
    }
    
    /// Create a lambda
    pub fn lambda(name: Option<Arc<str>>, body: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Lambda { name, body })
    }
    
    /// Create an application
    pub fn app(func: Arc<Term>, arg: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::App { func, arg })
    }
    
    /// Create a Sigma type
    pub fn sigma(name: Option<Arc<str>>, first: Arc<Term>, second: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Sigma { name, first, second })
    }
    
    /// Create a pair
    pub fn pair(first: Arc<Term>, second: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Pair { first, second })
    }
    
    /// Create first projection
    pub fn fst(term: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Fst(term))
    }
    
    /// Create second projection
    pub fn snd(term: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Snd(term))
    }
    
    /// Create identity type
    pub fn id(ty: Arc<Term>, left: Arc<Term>, right: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Id { ty, left, right })
    }
    
    /// Create reflexivity
    pub fn refl(term: Arc<Term>) -> Arc<Self> {
        Arc::new(Term::Refl(term))
    }
    
    /// Create universe
    pub fn universe(level: Level) -> Arc<Self> {
        Arc::new(Term::Universe(level))
    }
    
    /// Shift De Bruijn levels by delta above cutoff
    pub fn shift(&self, delta: i32, cutoff: Level) -> Arc<Term> {
        match self {
            Term::Var(level) => {
                if *level >= cutoff {
                    Arc::new(Term::Var((*level as i32 + delta) as Level))
                } else {
                    Arc::new(self.clone())
                }
            }
            Term::Pi { name, domain, codomain } => {
                Term::pi(
                    name.clone(),
                    domain.shift(delta, cutoff),
                    codomain.shift(delta, cutoff + 1),
                )
            }
            Term::Lambda { name, body } => {
                Term::lambda(name.clone(), body.shift(delta, cutoff + 1))
            }
            Term::App { func, arg } => {
                Term::app(func.shift(delta, cutoff), arg.shift(delta, cutoff))
            }
            Term::Sigma { name, first, second } => {
                Term::sigma(
                    name.clone(),
                    first.shift(delta, cutoff),
                    second.shift(delta, cutoff + 1),
                )
            }
            Term::Pair { first, second } => {
                Term::pair(first.shift(delta, cutoff), second.shift(delta, cutoff))
            }
            Term::Fst(term) => Term::fst(term.shift(delta, cutoff)),
            Term::Snd(term) => Term::snd(term.shift(delta, cutoff)),
            Term::Id { ty, left, right } => {
                Term::id(
                    ty.shift(delta, cutoff),
                    left.shift(delta, cutoff),
                    right.shift(delta, cutoff),
                )
            }
            Term::Refl(term) => Term::refl(term.shift(delta, cutoff)),
            Term::Universe(_) => Arc::new(self.clone()),
        }
    }
    
    /// Substitute term for variable at given level
    pub fn subst(&self, level: Level, replacement: &Arc<Term>) -> Arc<Term> {
        match self {
            Term::Var(var_level) => {
                if *var_level == level {
                    replacement.clone()
                } else {
                    Arc::new(self.clone())
                }
            }
            Term::Pi { name, domain, codomain } => {
                Term::pi(
                    name.clone(),
                    domain.subst(level, replacement),
                    codomain.subst(level + 1, &replacement.shift(1, 0)),
                )
            }
            Term::Lambda { name, body } => {
                Term::lambda(name.clone(), body.subst(level + 1, &replacement.shift(1, 0)))
            }
            Term::App { func, arg } => {
                Term::app(func.subst(level, replacement), arg.subst(level, replacement))
            }
            Term::Sigma { name, first, second } => {
                Term::sigma(
                    name.clone(),
                    first.subst(level, replacement),
                    second.subst(level + 1, &replacement.shift(1, 0)),
                )
            }
            Term::Pair { first, second } => {
                Term::pair(first.subst(level, replacement), second.subst(level, replacement))
            }
            Term::Fst(term) => Term::fst(term.subst(level, replacement)),
            Term::Snd(term) => Term::snd(term.subst(level, replacement)),
            Term::Id { ty, left, right } => {
                Term::id(
                    ty.subst(level, replacement),
                    left.subst(level, replacement),
                    right.subst(level, replacement),
                )
            }
            Term::Refl(term) => Term::refl(term.subst(level, replacement)),
            Term::Universe(_) => Arc::new(self.clone()),
        }
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Var(level) => write!(f, "#{}", level),
            Term::Pi { name, domain, codomain } => {
                match name {
                    Some(n) => write!(f, "({}: {}) -> {}", n, domain, codomain),
                    None => write!(f, "{} -> {}", domain, codomain),
                }
            }
            Term::Lambda { name, body } => {
                match name {
                    Some(n) => write!(f, "λ{}. {}", n, body),
                    None => write!(f, "λ. {}", body),
                }
            }
            Term::App { func, arg } => write!(f, "({} {})", func, arg),
            Term::Sigma { name, first, second } => {
                match name {
                    Some(n) => write!(f, "({}: {}) × {}", n, first, second),
                    None => write!(f, "{} × {}", first, second),
                }
            }
            Term::Pair { first, second } => write!(f, "({}, {})", first, second),
            Term::Fst(term) => write!(f, "π₁({})", term),
            Term::Snd(term) => write!(f, "π₂({})", term),
            Term::Id { ty, left, right } => write!(f, "{} ≡[{}] {}", left, ty, right),
            Term::Refl(term) => write!(f, "refl({})", term),
            Term::Universe(level) => write!(f, "Type{}", level),
        }
    }
}