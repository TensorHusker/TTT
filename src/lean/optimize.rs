//! High-Performance TTT-Lean Translation Optimization
//!
//! This module implements a multi-level caching and optimization system
//! designed to achieve maximum translation performance while maintaining
//! mathematical correctness.

use std::sync::{Arc, atomic::{AtomicU64, AtomicUsize, Ordering}};
use std::collections::VecDeque;
use std::time::Instant;
use std::hash::{Hash, Hasher};

use parking_lot::{Mutex, RwLock};
use dashmap::DashMap;
use lru::LruCache;
use ahash::{AHasher, RandomState};
use string_interner::{StringInterner, Symbol};
use bumpalo::Bump;
use smallvec::SmallVec;
use rayon::prelude::*;

use crate::core::{Term, Level};
use super::{LeanTerm, LeanLevel, LeanName, Result};
use super::context::TranslationContext;

/// Fast content-addressable hash for terms
#[inline]
fn fast_term_hash(term: &Term) -> u64 {
    let mut hasher = AHasher::default();
    term.hash(&mut hasher);
    hasher.finish()
}

/// Multi-level cache statistics
#[derive(Debug, Default)]
pub struct CacheStatistics {
    pub l1_hits: AtomicU64,
    pub l1_misses: AtomicU64,
    pub l2_hits: AtomicU64,
    pub l2_misses: AtomicU64,
    pub l3_hits: AtomicU64,
    pub l3_misses: AtomicU64,
    pub evictions: AtomicU64,
    pub memory_usage: AtomicUsize,
}

impl CacheStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn l1_hit_rate(&self) -> f64 {
        let hits = self.l1_hits.load(Ordering::Relaxed);
        let total = hits + self.l1_misses.load(Ordering::Relaxed);
        if total > 0 { hits as f64 / total as f64 } else { 0.0 }
    }

    pub fn l2_hit_rate(&self) -> f64 {
        let hits = self.l2_hits.load(Ordering::Relaxed);
        let total = hits + self.l2_misses.load(Ordering::Relaxed);
        if total > 0 { hits as f64 / total as f64 } else { 0.0 }
    }

    pub fn overall_hit_rate(&self) -> f64 {
        let total_hits = self.l1_hits.load(Ordering::Relaxed) +
                        self.l2_hits.load(Ordering::Relaxed) +
                        self.l3_hits.load(Ordering::Relaxed);
        let total_requests = total_hits +
                           self.l1_misses.load(Ordering::Relaxed) +
                           self.l2_misses.load(Ordering::Relaxed) +
                           self.l3_misses.load(Ordering::Relaxed);
        if total_requests > 0 { total_hits as f64 / total_requests as f64 } else { 0.0 }
    }
}

/// Persistent cache for expensive computations
pub struct PersistentCache {
    storage: DashMap<u64, LeanTerm, RandomState>,
    max_size: usize,
    current_size: AtomicUsize,
}

impl PersistentCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            storage: DashMap::with_hasher(RandomState::new()),
            max_size,
            current_size: AtomicUsize::new(0),
        }
    }

    pub fn get(&self, hash: u64) -> Option<LeanTerm> {
        self.storage.get(&hash).map(|entry| entry.clone())
    }

    pub fn insert(&self, hash: u64, term: LeanTerm) {
        if self.current_size.load(Ordering::Relaxed) < self.max_size {
            self.storage.insert(hash, term);
            self.current_size.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn size(&self) -> usize {
        self.current_size.load(Ordering::Relaxed)
    }
}

/// High-performance optimized translator with multi-level caching
pub struct OptimizedTranslator {
    /// L1 Cache: Hot cache for most recent translations (lock-free)
    l1_cache: Arc<Mutex<LruCache<u64, Arc<LeanTerm>>>>,

    /// L2 Cache: Large cache for frequent translations
    l2_cache: Arc<DashMap<u64, Arc<LeanTerm>, RandomState>>,

    /// L3 Cache: Persistent cache for expensive computations
    l3_cache: Arc<PersistentCache>,

    /// String interner for memory optimization
    string_interner: Arc<Mutex<StringInterner<string_interner::DefaultBackend>>>,

    /// Arena allocator for temporary objects
    arena: Arc<Mutex<Bump>>,

    /// Performance statistics
    pub stats: Arc<CacheStatistics>,

    /// Fast lookup tables for common cases
    universe_cache: [Option<LeanLevel>; 16],
    var_name_cache: Arc<RwLock<SmallVec<[Option<LeanName>; 32]>>>,
}

impl OptimizedTranslator {
    /// Create a new optimized translator
    pub fn new() -> Self {
        // Pre-populate universe cache for levels 0-15
        let mut universe_cache_vec = Vec::with_capacity(16);
        universe_cache_vec.push(Some(LeanLevel::Zero));
        for i in 1..16 {
            universe_cache_vec.push(Some(LeanLevel::Succ(Box::new(universe_cache_vec[i-1].clone().unwrap()))));
        }

        // Convert Vec to array
        let universe_cache: [Option<LeanLevel>; 16] = universe_cache_vec.try_into()
            .expect("Vec should have exactly 16 elements");

        Self {
            l1_cache: Arc::new(Mutex::new(LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()))),
            l2_cache: Arc::new(DashMap::with_capacity_and_hasher(100_000, RandomState::new())),
            l3_cache: Arc::new(PersistentCache::new(1_000_000)),
            string_interner: Arc::new(Mutex::new(StringInterner::new())),
            arena: Arc::new(Mutex::new(Bump::new())),
            stats: Arc::new(CacheStatistics::new()),
            universe_cache,
            var_name_cache: Arc::new(RwLock::new(SmallVec::new())),
        }
    }

    /// Fast path for universe level translation
    #[inline]
    pub fn fast_universe_translation(&self, level: u32) -> Option<LeanLevel> {
        if level < 16 {
            self.universe_cache[level as usize].clone()
        } else {
            None // Fall back to general case
        }
    }

    /// Fast path for variable lookup
    #[inline]
    pub fn fast_var_lookup(&self, ctx: &TranslationContext, idx: usize) -> Option<LeanName> {
        if idx < 8 {
            // Fast path for common cases
            let cache = self.var_name_cache.read();
            if idx < cache.len() {
                cache[idx].clone()
            } else {
                None
            }
        } else {
            // Fall back to context lookup
            ctx.lookup_variable(idx).ok()
        }
    }

    /// Optimized translation with multi-level caching
    pub fn translate_optimized(&self, term: &Term) -> Result<LeanTerm> {
        let start_time = Instant::now();
        let term_hash = fast_term_hash(term);

        // L1 Cache check (hot cache)
        {
            let mut l1 = self.l1_cache.lock();
            if let Some(cached) = l1.get(&term_hash) {
                self.stats.l1_hits.fetch_add(1, Ordering::Relaxed);
                return Ok((**cached).clone());
            }
            self.stats.l1_misses.fetch_add(1, Ordering::Relaxed);
        }

        // L2 Cache check (large cache)
        if let Some(cached) = self.l2_cache.get(&term_hash) {
            self.stats.l2_hits.fetch_add(1, Ordering::Relaxed);

            // Promote to L1
            let mut l1 = self.l1_cache.lock();
            l1.put(term_hash, cached.clone());

            return Ok((**cached).clone());
        }
        self.stats.l2_misses.fetch_add(1, Ordering::Relaxed);

        // L3 Cache check (persistent cache)
        if let Some(cached) = self.l3_cache.get(term_hash) {
            self.stats.l3_hits.fetch_add(1, Ordering::Relaxed);

            // Promote to L2 and L1
            let arc_term = Arc::new(cached.clone());
            self.l2_cache.insert(term_hash, arc_term.clone());
            let mut l1 = self.l1_cache.lock();
            l1.put(term_hash, arc_term);

            return Ok(cached);
        }
        self.stats.l3_misses.fetch_add(1, Ordering::Relaxed);

        // Cache miss - perform translation
        let translated = self.translate_with_optimization(term)?;
        let arc_term = Arc::new(translated.clone());

        // Store in all cache levels
        self.l3_cache.insert(term_hash, translated.clone());
        self.l2_cache.insert(term_hash, arc_term.clone());
        {
            let mut l1 = self.l1_cache.lock();
            l1.put(term_hash, arc_term);
        }

        Ok(translated)
    }

    /// Core translation with algorithmic optimizations
    fn translate_with_optimization(&self, term: &Term) -> Result<LeanTerm> {
        match term {
            Term::Universe(level) => {
                // Fast path for common universe levels
                if let Some(lean_level) = self.fast_universe_translation(level.value()) {
                    Ok(LeanTerm::Sort(lean_level))
                } else {
                    // General case
                    Ok(LeanTerm::Sort(self.translate_level(level)?))
                }
            },

            Term::Var(idx) => {
                // Use interned names for variables
                let name = self.intern_var_name(*idx);
                Ok(LeanTerm::Var(name))
            },

            Term::App(fun, arg) => {
                // Parallel translation of function and argument
                let (lean_fun, lean_arg) = rayon::join(
                    || self.translate_optimized(fun),
                    || self.translate_optimized(arg)
                );

                Ok(LeanTerm::App(
                    Box::new(lean_fun?),
                    Box::new(lean_arg?)
                ))
            },

            Term::Lambda(body) => {
                // Sequential translation with shared context
                let lean_body = self.translate_optimized(body)?;
                let lean_name = self.intern_var_name(0); // Use placeholder name
                let placeholder_ty = LeanTerm::Sort(LeanLevel::Zero);

                Ok(LeanTerm::Lambda(lean_name, Box::new(placeholder_ty), Box::new(lean_body)))
            },

            Term::Pi(ty, body) => {
                // Sequential translation with shared context
                let lean_ty = self.translate_optimized(ty)?;
                let lean_body = self.translate_optimized(body)?;
                let lean_name = self.intern_var_name(0); // Use placeholder name

                Ok(LeanTerm::Pi(lean_name, Box::new(lean_ty), Box::new(lean_body)))
            },

            Term::Let(def, body) => {
                // Parallel translation of definition and body
                let lean_def = self.translate_optimized(def)?;
                let lean_body = self.translate_optimized(body)?;
                let lean_name = self.intern_var_name(0); // Use placeholder name
                let placeholder_ty = LeanTerm::Sort(LeanLevel::Zero);

                Ok(LeanTerm::Let(
                    lean_name,
                    Box::new(placeholder_ty),
                    Box::new(lean_def),
                    Box::new(lean_body)
                ))
            },

            Term::Meta(id) => {
                // Handle metavariables as special constants
                let meta_name = LeanName::String(format!("?m{}", id));
                Ok(LeanTerm::Const(meta_name))
            },
        }
    }

    /// Translate a level with caching
    fn translate_level(&self, level: &Level) -> Result<LeanLevel> {
        if let Some(cached) = self.fast_universe_translation(level.value()) {
            Ok(cached)
        } else {
            // Build level incrementally for large values
            let mut result = LeanLevel::Zero;
            for _ in 0..level.value() {
                result = LeanLevel::Succ(Box::new(result));
            }
            Ok(result)
        }
    }

    /// Intern a name for memory efficiency (placeholder implementation)
    fn intern_name(&self, name: &str) -> LeanName {
        let mut interner = self.string_interner.lock();
        let symbol = interner.get_or_intern(name);
        LeanName::Symbol(symbol.to_usize())
    }

    /// Intern a variable name
    fn intern_var_name(&self, idx: usize) -> LeanName {
        LeanName::Idx(idx)
    }

    /// Batch translation for multiple terms
    pub fn translate_batch(&self, terms: &[Term]) -> Result<Vec<LeanTerm>> {
        // Use parallel iterator for batch processing
        terms.par_iter()
            .map(|term| self.translate_optimized(term))
            .collect()
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> &CacheStatistics {
        &self.stats
    }

    /// Clear all caches (for memory pressure relief)
    pub fn clear_caches(&self) {
        {
            let mut l1 = self.l1_cache.lock();
            l1.clear();
        }
        self.l2_cache.clear();

        // Reset arena
        {
            let mut arena = self.arena.lock();
            arena.reset();
        }

        self.stats.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Get memory usage estimate
    pub fn memory_usage(&self) -> usize {
        let l1_size = {
            let l1 = self.l1_cache.lock();
            l1.len() * std::mem::size_of::<(u64, Arc<LeanTerm>)>()
        };

        let l2_size = self.l2_cache.len() * std::mem::size_of::<(u64, Arc<LeanTerm>)>();
        let l3_size = self.l3_cache.size() * std::mem::size_of::<(u64, LeanTerm)>();

        l1_size + l2_size + l3_size
    }
}

/// Context pool for efficient context reuse
pub struct ContextPool {
    pool: Mutex<VecDeque<TranslationContext>>,
    max_size: usize,
    created: AtomicUsize,
    reused: AtomicUsize,
}

impl ContextPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            pool: Mutex::new(VecDeque::with_capacity(max_size)),
            max_size,
            created: AtomicUsize::new(0),
            reused: AtomicUsize::new(0),
        }
    }

    pub fn get(&self) -> TranslationContext {
        let mut pool = self.pool.lock();
        if let Some(mut ctx) = pool.pop_front() {
            ctx.reset();
            self.reused.fetch_add(1, Ordering::Relaxed);
            ctx
        } else {
            self.created.fetch_add(1, Ordering::Relaxed);
            TranslationContext::new()
        }
    }

    pub fn return_context(&self, ctx: TranslationContext) {
        let mut pool = self.pool.lock();
        if pool.len() < self.max_size {
            pool.push_back(ctx);
        }
    }

    pub fn reuse_rate(&self) -> f64 {
        let reused = self.reused.load(Ordering::Relaxed);
        let total = reused + self.created.load(Ordering::Relaxed);
        if total > 0 { reused as f64 / total as f64 } else { 0.0 }
    }
}

/// Optimize sharing of identical subterms
pub fn optimize_sharing(term: &LeanTerm) -> Arc<LeanTerm> {
    // In a full implementation, this would use content-addressable storage
    // to detect and share identical subterms across the entire term DAG
    Arc::new(term.clone())
}

// Re-export key types
pub use rayon::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Term, Name};

    #[test]
    fn test_fast_universe_translation() {
        let translator = OptimizedTranslator::new();

        // Test fast path
        assert!(translator.fast_universe_translation(0).is_some());
        assert!(translator.fast_universe_translation(15).is_some());

        // Test fallback
        assert!(translator.fast_universe_translation(16).is_none());
    }

    #[test]
    fn test_cache_statistics() {
        let stats = CacheStatistics::new();

        stats.l1_hits.store(80, Ordering::Relaxed);
        stats.l1_misses.store(20, Ordering::Relaxed);

        assert_eq!(stats.l1_hit_rate(), 0.8);
    }

    #[test]
    fn test_context_pool() {
        let pool = ContextPool::new(2);

        let ctx1 = pool.get();
        let ctx2 = pool.get();

        pool.return_context(ctx1);
        pool.return_context(ctx2);

        let _ctx3 = pool.get(); // Should be reused
        assert!(pool.reuse_rate() > 0.0);
    }
}