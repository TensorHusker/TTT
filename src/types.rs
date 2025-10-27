//! Type system with Universe hierarchy via phantom types

use crate::{term::Term, Level};
use alloc::sync::Arc;
use core::marker::PhantomData;

/// Universe levels as phantom types for zero-cost abstractions
pub struct Universe<const LEVEL: u32>;

/// Type alias for the type representation
pub type Type = Term;

/// Type environment for context management
#[derive(Debug, Clone)]
pub struct TypeEnv {
    /// Bindings from De Bruijn level to type
    bindings: alloc::vec::Vec<Arc<Type>>,
}

impl TypeEnv {
    /// Create empty environment
    pub fn new() -> Self {
        Self {
            bindings: alloc::vec::Vec::new(),
        }
    }
    
    /// Get type at level
    pub fn get(&self, level: Level) -> Option<&Arc<Type>> {
        self.bindings.get(level as usize)
    }
    
    /// Extend environment with new binding
    pub fn extend(&mut self, ty: Arc<Type>) -> Level {
        let level = self.bindings.len() as Level;
        self.bindings.push(ty);
        level
    }
    
    /// Current environment size
    pub fn size(&self) -> Level {
        self.bindings.len() as Level
    }
    
    /// Shift all types in environment
    pub fn shift(&self, delta: i32, cutoff: Level) -> Self {
        Self {
            bindings: self.bindings
                .iter()
                .map(|ty| ty.shift(delta, cutoff))
                .collect(),
        }
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Universe predicates using const generics
impl<const LEVEL: u32> Universe<LEVEL> {
    /// Check if a type belongs to this universe level
    pub fn contains(ty: &Type) -> bool {
        match ty {
            Term::Universe(level) => *level < LEVEL,
            Term::Pi { domain, codomain, .. } => {
                Self::contains(domain) && Universe::<LEVEL>::contains(codomain)
            }
            Term::Sigma { first, second, .. } => {
                Self::contains(first) && Universe::<LEVEL>::contains(second)
            }
            Term::Id { ty, .. } => Self::contains(ty),
            _ => false,
        }
    }
    
    /// Get the universe level
    pub const fn level() -> u32 {
        LEVEL
    }
    
    /// Create a term representing this universe
    pub fn term() -> Arc<Type> {
        Term::universe(LEVEL)
    }
}

/// Type alias for common universe levels
pub type Type0 = Universe<0>;
pub type Type1 = Universe<1>;
pub type Type2 = Universe<2>;

/// Phantom type marker for proof irrelevance
pub struct Proof<T> {
    _phantom: PhantomData<T>,
}

impl<T> Proof<T> {
    /// Create a proof witness (zero-cost)
    pub const fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> Clone for Proof<T> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl<T> Copy for Proof<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_universe_hierarchy() {
        let type0 = Term::universe(0);
        let type1 = Term::universe(1);
        
        assert!(Type1::contains(&type0));
        assert!(!Type0::contains(&type1));
        assert!(Type2::contains(&type1));
    }
    
    #[test]
    fn test_env_operations() {
        let mut env = TypeEnv::new();
        let ty = Term::universe(0);
        
        let level = env.extend(ty.clone());
        assert_eq!(level, 0);
        assert_eq!(env.get(0), Some(&ty));
        assert_eq!(env.size(), 1);
    }
}