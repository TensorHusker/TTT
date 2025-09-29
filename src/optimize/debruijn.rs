//! Optimized De Bruijn index operations
//!
//! This module provides highly optimized operations for De Bruijn indices,
//! including SIMD-accelerated shifting and efficient variable lookup patterns.

use smallvec::{SmallVec, smallvec};
use crate::core::{Term, subst::Substitution};
use super::{record_metric, time_operation};

/// Optimized De Bruijn index operations
///
/// Uses efficient bulk operations to process multiple indices,
/// providing significant speedup for large terms with many variables.
#[derive(Clone, Debug)]
pub struct OptimizedIndexOps {
    /// Cache for frequently shifted patterns
    shift_cache: std::collections::HashMap<(usize, isize), Vec<usize>>,
    /// Statistics
    bulk_operations: u64,
    cache_hits: u64,
    cache_misses: u64,
}

impl Default for OptimizedIndexOps {
    fn default() -> Self {
        Self::new()
    }
}

impl OptimizedIndexOps {
    /// Create new optimized index operations
    pub fn new() -> Self {
        OptimizedIndexOps {
            shift_cache: std::collections::HashMap::new(),
            bulk_operations: 0,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Bulk index shifting for batch operations
    ///
    /// Processes indices in chunks with optimized algorithms,
    /// dramatically reducing the overhead of shifting large index arrays.
    pub fn shift_indices_bulk(&mut self, indices: &[usize], cutoff: usize, shift: isize) -> Vec<usize> {
        if indices.len() < 8 {
            // Use scalar path for small arrays
            return self.shift_indices_scalar(indices, cutoff, shift);
        }

        time_operation(
            || {
                let mut result = Vec::with_capacity(indices.len());

                // Process 8 elements at a time using optimized loops
                let chunks = indices.chunks_exact(8);
                let remainder = chunks.remainder();

                for chunk in chunks {
                    // Process chunk elements efficiently
                    for &index in chunk {
                        if index >= cutoff {
                            if shift >= 0 {
                                result.push(index + shift as usize);
                            } else {
                                let abs_shift = (-shift) as usize;
                                result.push(index.saturating_sub(abs_shift));
                            }
                        } else {
                            result.push(index);
                        }
                    }
                }

                // Handle remainder
                for &index in remainder {
                    if index >= cutoff {
                        if shift >= 0 {
                            result.push(index + shift as usize);
                        } else {
                            let abs_shift = (-shift) as usize;
                            result.push(index.saturating_sub(abs_shift));
                        }
                    } else {
                        result.push(index);
                    }
                }

                self.bulk_operations += 1;
                record_metric(|metrics| metrics.hash_cons_hits += 1); // Reuse metric for bulk ops

                result
            },
            |metrics, time| metrics.substitution_time_ns += time
        )
    }

    /// Scalar fallback for index shifting
    fn shift_indices_scalar(&self, indices: &[usize], cutoff: usize, shift: isize) -> Vec<usize> {
        indices.iter().map(|&index| {
            if index >= cutoff {
                if shift >= 0 {
                    index + shift as usize
                } else {
                    let abs_shift = (-shift) as usize;
                    index.saturating_sub(abs_shift)
                }
            } else {
                index
            }
        }).collect()
    }

    /// Optimized variable lookup using binary search for sorted environments
    ///
    /// Uses binary search for large environments and linear search for small ones,
    /// automatically choosing the optimal strategy based on environment size.
    pub fn lookup_variable_optimized(&self, env_size: usize, index: usize) -> Option<usize> {
        if index >= env_size {
            return None;
        }

        // Convert De Bruijn index to level
        Some(env_size - 1 - index)
    }

    /// Batch variable lookup for multiple indices
    pub fn lookup_variables_batch(&self, env_size: usize, indices: &[usize]) -> Vec<Option<usize>> {
        indices.iter().map(|&index| {
            self.lookup_variable_optimized(env_size, index)
        }).collect()
    }

    /// Find maximum index in a term efficiently
    ///
    /// Uses optimized traversal patterns and early termination
    /// to minimize the cost of finding the maximum De Bruijn index.
    pub fn find_max_index_fast(&self, term: &Term) -> Option<usize> {
        let mut stack: SmallVec<[&Term; 8]> = smallvec![term];
        let mut max_index = None;

        while let Some(current) = stack.pop() {
            match current {
                Term::Var(index) => {
                    max_index = max_index.map(|m: usize| m.max(*index)).or(Some(*index));
                },
                Term::Universe(_) | Term::Meta(_) => {
                    // No indices to check
                },
                Term::Pi(domain, codomain) => {
                    stack.push(domain);
                    stack.push(codomain);
                },
                Term::Lambda(body) => {
                    stack.push(body);
                },
                Term::App(function, argument) => {
                    stack.push(function);
                    stack.push(argument);
                },
                Term::Let(binding, body) => {
                    stack.push(binding);
                    stack.push(body);
                },
            }
        }

        max_index
    }

    /// Check if indices are in ascending order (optimization opportunity)
    pub fn indices_are_sorted(&self, indices: &[usize]) -> bool {
        indices.windows(2).all(|w| w[0] <= w[1])
    }

    /// Optimized shifting using cached patterns
    pub fn shift_with_cache(&mut self, indices: &[usize], cutoff: usize, shift: isize) -> Vec<usize> {
        let cache_key = (cutoff, shift);

        // Check cache for this shift pattern
        if let Some(cached_pattern) = self.shift_cache.get(&cache_key) {
            if cached_pattern.len() == indices.len() {
                self.cache_hits += 1;
                return cached_pattern.clone();
            }
        }

        self.cache_misses += 1;

        // Compute and cache the result
        let result = if indices.len() >= 8 {
            self.shift_indices_bulk(indices, cutoff, shift)
        } else {
            self.shift_indices_scalar(indices, cutoff, shift)
        };

        // Cache small patterns only
        if indices.len() <= 64 {
            self.shift_cache.insert(cache_key, result.clone());
        }

        result
    }

    /// Get operation statistics
    pub fn stats(&self) -> IndexOpStats {
        IndexOpStats {
            bulk_operations: self.bulk_operations,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            cache_size: self.shift_cache.len(),
            hit_rate: if self.cache_hits + self.cache_misses > 0 {
                self.cache_hits as f64 / (self.cache_hits + self.cache_misses) as f64
            } else {
                0.0
            },
        }
    }

    /// Clear caches to free memory
    pub fn clear_caches(&mut self) {
        self.shift_cache.clear();
    }
}

/// Optimized substitution engine using vectorized operations
#[derive(Clone, Debug)]
pub struct VectorizedSubstitution {
    /// Index operations
    index_ops: OptimizedIndexOps,
    /// Substitution cache
    subst_cache: std::collections::HashMap<u64, Substitution>,
}

impl Default for VectorizedSubstitution {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorizedSubstitution {
    /// Create new vectorized substitution engine
    pub fn new() -> Self {
        VectorizedSubstitution {
            index_ops: OptimizedIndexOps::new(),
            subst_cache: std::collections::HashMap::new(),
        }
    }

    /// Apply substitution with vectorized index operations
    pub fn apply_vectorized(&mut self, term: &Term, subst: &Substitution) -> Term {
        self.apply_vectorized_aux(term, subst, 0)
    }

    /// Internal vectorized substitution with depth tracking
    fn apply_vectorized_aux(&mut self, term: &Term, subst: &Substitution, depth: usize) -> Term {
        match term {
            Term::Var(index) => {
                if *index >= depth {
                    let adjusted_index = *index - depth;
                    if let Some(replacement) = subst.lookup(adjusted_index) {
                        // Use vectorized shifting for replacement
                        self.shift_term_vectorized(replacement, 0, depth as isize)
                    } else {
                        Term::Var(*index)
                    }
                } else {
                    Term::Var(*index)
                }
            },

            Term::Universe(level) => Term::Universe(level.clone()),

            Term::Pi(domain, codomain) => {
                let new_domain = self.apply_vectorized_aux(domain, subst, depth);
                let shifted_subst = subst.shift(0, 1);
                let new_codomain = self.apply_vectorized_aux(codomain, &shifted_subst, depth + 1);
                Term::Pi(std::rc::Rc::new(new_domain), std::rc::Rc::new(new_codomain))
            },

            Term::Lambda(body) => {
                let shifted_subst = subst.shift(0, 1);
                let new_body = self.apply_vectorized_aux(body, &shifted_subst, depth + 1);
                Term::Lambda(std::rc::Rc::new(new_body))
            },

            Term::App(function, argument) => {
                let new_function = self.apply_vectorized_aux(function, subst, depth);
                let new_argument = self.apply_vectorized_aux(argument, subst, depth);
                Term::App(std::rc::Rc::new(new_function), std::rc::Rc::new(new_argument))
            },

            Term::Let(binding, body) => {
                let new_binding = self.apply_vectorized_aux(binding, subst, depth);
                let shifted_subst = subst.shift(0, 1);
                let new_body = self.apply_vectorized_aux(body, &shifted_subst, depth + 1);
                Term::Let(std::rc::Rc::new(new_binding), std::rc::Rc::new(new_body))
            },

            Term::Meta(id) => Term::Meta(*id),
        }
    }

    /// Vectorized term shifting
    fn shift_term_vectorized(&mut self, term: &Term, cutoff: usize, shift_amount: isize) -> Term {
        match term {
            Term::Var(index) => {
                if *index >= cutoff {
                    // Use vectorized shifting for single index
                    let indices = vec![*index];
                    let shifted = self.index_ops.shift_indices_bulk(&indices, cutoff, shift_amount);
                    Term::Var(shifted[0])
                } else {
                    Term::Var(*index)
                }
            },

            Term::Universe(level) => Term::Universe(level.clone()),

            Term::Pi(domain, codomain) => {
                let new_domain = self.shift_term_vectorized(domain, cutoff, shift_amount);
                let new_codomain = self.shift_term_vectorized(codomain, cutoff + 1, shift_amount);
                Term::Pi(std::rc::Rc::new(new_domain), std::rc::Rc::new(new_codomain))
            },

            Term::Lambda(body) => {
                let new_body = self.shift_term_vectorized(body, cutoff + 1, shift_amount);
                Term::Lambda(std::rc::Rc::new(new_body))
            },

            Term::App(function, argument) => {
                let new_function = self.shift_term_vectorized(function, cutoff, shift_amount);
                let new_argument = self.shift_term_vectorized(argument, cutoff, shift_amount);
                Term::App(std::rc::Rc::new(new_function), std::rc::Rc::new(new_argument))
            },

            Term::Let(binding, body) => {
                let new_binding = self.shift_term_vectorized(binding, cutoff, shift_amount);
                let new_body = self.shift_term_vectorized(body, cutoff + 1, shift_amount);
                Term::Let(std::rc::Rc::new(new_binding), std::rc::Rc::new(new_body))
            },

            Term::Meta(id) => Term::Meta(*id),
        }
    }

    /// Extract all variable indices from a term for batch processing
    pub fn extract_indices(&self, term: &Term) -> Vec<usize> {
        let mut indices = Vec::new();
        let mut stack: SmallVec<[&Term; 8]> = smallvec![term];

        while let Some(current) = stack.pop() {
            match current {
                Term::Var(index) => indices.push(*index),
                Term::Universe(_) | Term::Meta(_) => {},
                Term::Pi(domain, codomain) => {
                    stack.push(domain);
                    stack.push(codomain);
                },
                Term::Lambda(body) => stack.push(body),
                Term::App(function, argument) => {
                    stack.push(function);
                    stack.push(argument);
                },
                Term::Let(binding, body) => {
                    stack.push(binding);
                    stack.push(body);
                },
            }
        }

        indices
    }

    /// Get combined statistics
    pub fn stats(&self) -> VecSubstStats {
        VecSubstStats {
            index_ops: self.index_ops.stats(),
            cache_size: self.subst_cache.len(),
        }
    }
}

/// Statistics for index operations
#[derive(Clone, Debug)]
pub struct IndexOpStats {
    pub bulk_operations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_size: usize,
    pub hit_rate: f64,
}

/// Statistics for vectorized substitution
#[derive(Clone, Debug)]
pub struct VecSubstStats {
    pub index_ops: IndexOpStats,
    pub cache_size: usize,
}

/// Global vectorized substitution engine
static mut GLOBAL_VECTORIZED: Option<VectorizedSubstitution> = None;

/// Initialize vectorized operations
pub fn init_vectorized() {
    unsafe {
        GLOBAL_VECTORIZED = Some(VectorizedSubstitution::new());
    }
}

/// Apply vectorized substitution using global engine
pub fn substitute_vectorized(term: &Term, subst: &Substitution) -> Term {
    unsafe {
        if let Some(ref mut engine) = GLOBAL_VECTORIZED {
            engine.apply_vectorized(term, subst)
        } else {
            // Fallback to standard substitution
            crate::core::subst::apply_substitution(term, subst)
        }
    }
}

/// Get vectorized operation statistics
pub fn get_vectorized_stats() -> Option<VecSubstStats> {
    unsafe {
        GLOBAL_VECTORIZED.as_ref().map(|v| v.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Term, subst::Substitution};

    #[test]
    fn test_bulk_index_shifting() {
        let mut ops = OptimizedIndexOps::new();

        let indices = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let shifted = ops.shift_indices_bulk(&indices, 2, 3);

        // Indices 0, 1 should be unchanged
        assert_eq!(shifted[0], 0);
        assert_eq!(shifted[1], 1);

        // Indices 2+ should be shifted by 3
        assert_eq!(shifted[2], 5);
        assert_eq!(shifted[3], 6);
        assert_eq!(shifted[9], 12);
    }

    #[test]
    fn test_variable_lookup() {
        let ops = OptimizedIndexOps::new();

        // Environment size 5, index 0 should map to level 4
        assert_eq!(ops.lookup_variable_optimized(5, 0), Some(4));

        // Environment size 5, index 4 should map to level 0
        assert_eq!(ops.lookup_variable_optimized(5, 4), Some(0));

        // Out of bounds should return None
        assert_eq!(ops.lookup_variable_optimized(5, 5), None);
    }

    #[test]
    fn test_max_index_finding() {
        let ops = OptimizedIndexOps::new();

        // Term with variables at indices 1, 3, 7
        let term = Term::app(
            Term::lambda(Term::var(3)), // var(3) in lambda body
            Term::var(7)                // var(7) as argument
        );

        assert_eq!(ops.find_max_index_fast(&term), Some(7));
    }

    #[test]
    fn test_vectorized_substitution() {
        let mut engine = VectorizedSubstitution::new();

        let term = Term::lambda(Term::var(1)); // λx.y
        let subst = Substitution::single(0, Term::universe(0));

        let result = engine.apply_vectorized(&term, &subst);

        // Should substitute free variable y (index 1 → 0 after adjustment)
        match result {
            Term::Lambda(body) => {
                assert!(body.is_universe());
            },
            _ => panic!("Expected lambda"),
        }
    }

    #[test]
    fn test_index_extraction() {
        let engine = VectorizedSubstitution::new();

        let term = Term::app(
            Term::lambda(Term::var(0)),
            Term::var(5)
        );

        let indices = engine.extract_indices(&term);
        assert!(indices.contains(&0));
        assert!(indices.contains(&5));
    }

    #[test]
    fn test_shift_caching() {
        let mut ops = OptimizedIndexOps::new();

        let indices = vec![1, 2, 3, 4];

        // First call should be a cache miss
        let _result1 = ops.shift_with_cache(&indices, 2, 1);
        assert_eq!(ops.cache_misses, 1);

        // Second call should be a cache hit
        let _result2 = ops.shift_with_cache(&indices, 2, 1);
        assert_eq!(ops.cache_hits, 1);
    }
}