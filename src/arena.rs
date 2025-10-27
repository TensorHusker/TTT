//! Arena-based memory management for term sharing

use crate::term::Term;
use alloc::{sync::Arc, vec::Vec};
use core::hash::{Hash, Hasher};

/// Arena for sharing term instances
#[derive(Debug, Default)]
pub struct Arena {
    /// Interned terms for sharing
    terms: Vec<Arc<Term>>,
}

/// Handle to a term in the arena
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermHandle {
    id: usize,
}

impl Arena {
    /// Create a new arena
    pub fn new() -> Self {
        Self {
            terms: Vec::new(),
        }
    }
    
    /// Intern a term and return its handle
    pub fn intern(&mut self, term: Term) -> TermHandle {
        let arc_term = Arc::new(term);
        
        // Check if term already exists (linear search for simplicity)
        for (id, existing) in self.terms.iter().enumerate() {
            if Arc::ptr_eq(existing, &arc_term) || **existing == *arc_term {
                return TermHandle { id };
            }
        }
        
        // Add new term
        let id = self.terms.len();
        self.terms.push(arc_term);
        TermHandle { id }
    }
    
    /// Get term by handle
    pub fn get(&self, handle: &TermHandle) -> Option<&Arc<Term>> {
        self.terms.get(handle.id)
    }
    
    /// Get the underlying Arc directly (zero-cost)
    pub fn get_arc(&self, handle: &TermHandle) -> Option<Arc<Term>> {
        self.terms.get(handle.id).cloned()
    }
    
    /// Number of interned terms
    pub fn len(&self) -> usize {
        self.terms.len()
    }
    
    /// Check if arena is empty
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
    
    /// Clear all terms (useful for testing)
    pub fn clear(&mut self) {
        self.terms.clear();
    }
    
    /// Iterator over all terms
    pub fn iter(&self) -> impl Iterator<Item = &Arc<Term>> {
        self.terms.iter()
    }
}

impl Hash for TermHandle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::Term;
    
    #[test]
    fn test_arena_basic() {
        let mut arena = Arena::new();
        let term1 = Term::Var(0);
        let term2 = Term::Var(0);
        
        let handle1 = arena.intern(term1);
        let handle2 = arena.intern(term2);
        
        // Same terms should get same handle due to sharing
        assert_eq!(handle1, handle2);
        assert_eq!(arena.len(), 1);
    }
    
    #[test]
    fn test_arena_different_terms() {
        let mut arena = Arena::new();
        let term1 = Term::Var(0);
        let term2 = Term::Var(1);
        
        let handle1 = arena.intern(term1);
        let handle2 = arena.intern(term2);
        
        assert_ne!(handle1, handle2);
        assert_eq!(arena.len(), 2);
        
        let retrieved1 = arena.get(&handle1).unwrap();
        let retrieved2 = arena.get(&handle2).unwrap();
        
        assert_eq!(**retrieved1, Term::Var(0));
        assert_eq!(**retrieved2, Term::Var(1));
    }
}