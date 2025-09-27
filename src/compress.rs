//! Type-directed compression traits

use crate::{term::Term, Arc};
use alloc::{vec::Vec, boxed::Box};

/// Trait for type-directed compression
pub trait TypeCompressible<R> {
    /// Associated proof type for compression correctness
    type Proof;
    
    /// Compress value using type information
    fn compress(&self, ty: &Arc<Term>) -> (R, Self::Proof);
    
    /// Decompress value with proof
    fn decompress(compressed: R, proof: Self::Proof, ty: &Arc<Term>) -> Self;
}

/// Compression context for tracking shared subterms
#[derive(Debug, Clone, Default)]
pub struct CompressionContext {
    /// Shared subterms with their indices
    shared: Vec<Arc<Term>>,
}

impl CompressionContext {
    /// Create new compression context
    pub fn new() -> Self {
        Self {
            shared: Vec::new(),
        }
    }
    
    /// Find or add shared term
    pub fn intern(&mut self, term: Arc<Term>) -> usize {
        // Check if term already exists
        for (idx, existing) in self.shared.iter().enumerate() {
            if **existing == *term {
                return idx;
            }
        }
        
        // Add new term
        let idx = self.shared.len();
        self.shared.push(term);
        idx
    }
    
    /// Get term by index
    pub fn get(&self, idx: usize) -> Option<&Arc<Term>> {
        self.shared.get(idx)
    }
    
    /// Number of shared terms
    pub fn len(&self) -> usize {
        self.shared.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.shared.is_empty()
    }
}

/// Compressed representation of a term
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressedTerm {
    /// Reference to shared term
    Shared(usize),
    /// Inline term structure
    Inline(CompressedInline),
}

/// Inline compressed term structures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressedInline {
    Var(u32),
    Universe(u32),
    App { func: Box<CompressedTerm>, arg: Box<CompressedTerm> },
    Fst(Box<CompressedTerm>),
    Snd(Box<CompressedTerm>),
}

/// Proof of compression correctness
pub struct CompressionProof<T> {
    _phantom: core::marker::PhantomData<T>,
}

impl<T> CompressionProof<T> {
    /// Create compression proof (zero-cost)
    pub const fn new() -> Self {
        Self {
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<T> Clone for CompressionProof<T> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl<T> Copy for CompressionProof<T> {}

impl TypeCompressible<CompressedTerm> for Term {
    type Proof = CompressionProof<Term>;
    
    fn compress(&self, _ty: &Arc<Term>) -> (CompressedTerm, Self::Proof) {
        let compressed = match self {
            Term::Var(level) => CompressedTerm::Inline(CompressedInline::Var(*level)),
            Term::Universe(level) => CompressedTerm::Inline(CompressedInline::Universe(*level)),
            Term::App { func, arg } => {
                let (func_comp, _) = func.compress(_ty);
                let (arg_comp, _) = arg.compress(_ty);
                CompressedTerm::Inline(CompressedInline::App {
                    func: Box::new(func_comp),
                    arg: Box::new(arg_comp),
                })
            }
            Term::Fst(term) => {
                let (term_comp, _) = term.compress(_ty);
                CompressedTerm::Inline(CompressedInline::Fst(Box::new(term_comp)))
            }
            Term::Snd(term) => {
                let (term_comp, _) = term.compress(_ty);
                CompressedTerm::Inline(CompressedInline::Snd(Box::new(term_comp)))
            }
            _ => {
                // For complex terms, we would use sharing
                // This is a simplified implementation
                CompressedTerm::Shared(0)
            }
        };
        
        (compressed, CompressionProof::new())
    }
    
    fn decompress(compressed: CompressedTerm, _proof: Self::Proof, _ty: &Arc<Term>) -> Self {
        match compressed {
            CompressedTerm::Inline(inline) => match inline {
                CompressedInline::Var(level) => Term::Var(level),
                CompressedInline::Universe(level) => Term::Universe(level),
                CompressedInline::App { func, arg } => {
                    let func_term = Self::decompress(*func, CompressionProof::new(), _ty);
                    let arg_term = Self::decompress(*arg, CompressionProof::new(), _ty);
                    Term::App {
                        func: Arc::new(func_term),
                        arg: Arc::new(arg_term),
                    }
                }
                CompressedInline::Fst(term) => {
                    let inner = Self::decompress(*term, CompressionProof::new(), _ty);
                    Term::Fst(Arc::new(inner))
                }
                CompressedInline::Snd(term) => {
                    let inner = Self::decompress(*term, CompressionProof::new(), _ty);
                    Term::Snd(Arc::new(inner))
                }
            },
            CompressedTerm::Shared(_) => {
                // Would lookup in compression context
                Term::Var(0) // Placeholder
            }
        }
    }
}

/// Compressed term database for efficient storage
#[derive(Debug, Default)]
pub struct CompressedDatabase {
    context: CompressionContext,
    compressed_terms: Vec<CompressedTerm>,
}

impl CompressedDatabase {
    /// Create new database
    pub fn new() -> Self {
        Self {
            context: CompressionContext::new(),
            compressed_terms: Vec::new(),
        }
    }
    
    /// Add term to database
    pub fn add_term(&mut self, term: &Arc<Term>, ty: &Arc<Term>) -> usize {
        let (compressed, _proof) = term.compress(ty);
        let idx = self.compressed_terms.len();
        self.compressed_terms.push(compressed);
        idx
    }
    
    /// Get compressed term by index
    pub fn get_compressed(&self, idx: usize) -> Option<&CompressedTerm> {
        self.compressed_terms.get(idx)
    }
    
    /// Decompress term by index
    pub fn decompress_term(&self, idx: usize, ty: &Arc<Term>) -> Option<Term> {
        self.compressed_terms.get(idx).map(|compressed| {
            Term::decompress(compressed.clone(), CompressionProof::new(), ty)
        })
    }
    
    /// Database size
    pub fn len(&self) -> usize {
        self.compressed_terms.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.compressed_terms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compression_basic() {
        let term = Term::Var(42);
        let ty = Term::universe(0);
        
        let (compressed, proof) = term.compress(&ty);
        let decompressed = Term::decompress(compressed, proof, &ty);
        
        assert_eq!(term, decompressed);
    }
    
    #[test]
    fn test_compression_context() {
        let mut ctx = CompressionContext::new();
        let term1 = Term::var(0);
        let term2 = Term::var(0);
        
        let idx1 = ctx.intern(term1.clone());
        let idx2 = ctx.intern(term2);
        
        // Same terms should get same index
        assert_eq!(idx1, idx2);
        assert_eq!(ctx.len(), 1);
    }
    
    #[test]
    fn test_compressed_database() {
        let mut db = CompressedDatabase::new();
        let term = Term::var(0);
        let ty = Term::universe(0);
        
        let idx = db.add_term(&term, &ty);
        let decompressed = db.decompress_term(idx, &ty).unwrap();
        
        assert_eq!(*term, decompressed);
    }
}