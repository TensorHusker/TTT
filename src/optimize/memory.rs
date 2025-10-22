//! Memory optimization using arena allocation and hash-consing
//!
//! This module provides advanced memory management techniques to reduce allocation
//! overhead and enable structural sharing of identical terms through hash-consing.

use std::sync::Arc;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use crate::core::{Term, Value};
use super::{record_metric, get_config};

/// Arena allocator for efficient term allocation
///
/// Allocates terms in large blocks to reduce allocation overhead and
/// improve memory locality. Terms allocated in the same arena are
/// freed together, enabling efficient memory management.
#[derive(Debug)]
pub struct TermArena {
    /// Current allocation block
    current_block: Vec<u8>,
    /// Position in current block
    current_pos: usize,
    /// Size of allocation blocks
    block_size: usize,
    /// All allocated blocks for cleanup
    blocks: Vec<Vec<u8>>,
    /// Statistics
    total_allocated: usize,
    num_allocations: usize,
}

impl TermArena {
    /// Create a new arena with specified block size
    pub fn new(block_size: usize) -> Self {
        let mut arena = TermArena {
            current_block: Vec::with_capacity(block_size),
            current_pos: 0,
            block_size,
            blocks: Vec::new(),
            total_allocated: 0,
            num_allocations: 0,
        };
        arena.current_block.resize(block_size, 0);
        arena
    }

    /// Allocate space for a term
    pub fn allocate<T>(&mut self, value: T) -> &mut T {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();

        // Align current position
        let aligned_pos = (self.current_pos + align - 1) & !(align - 1);

        // Check if we need a new block
        if aligned_pos + size > self.block_size {
            self.allocate_new_block();
            // Recalculate aligned position in new block
            let aligned_pos = (self.current_pos + align - 1) & !(align - 1);
            self.current_pos = aligned_pos;
        } else {
            self.current_pos = aligned_pos;
        }

        // Allocate in current block
        let ptr = &mut self.current_block[self.current_pos] as *mut u8 as *mut T;
        self.current_pos += size;
        self.total_allocated += size;
        self.num_allocations += 1;

        record_metric(|metrics| {
            metrics.bytes_allocated += size as u64;
            metrics.terms_allocated += 1;
        });

        unsafe {
            std::ptr::write(ptr, value);
            &mut *ptr
        }
    }

    /// Allocate a new block
    fn allocate_new_block(&mut self) {
        // Move current block to storage
        let old_block = std::mem::replace(&mut self.current_block, Vec::with_capacity(self.block_size));
        self.blocks.push(old_block);

        // Initialize new block
        self.current_block.resize(self.block_size, 0);
        self.current_pos = 0;
    }

    /// Get allocation statistics
    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            total_allocated: self.total_allocated,
            num_allocations: self.num_allocations,
            num_blocks: self.blocks.len() + 1,
            block_size: self.block_size,
            current_block_used: self.current_pos,
            fragmentation: self.calculate_fragmentation(),
        }
    }

    /// Calculate memory fragmentation ratio
    fn calculate_fragmentation(&self) -> f64 {
        let total_capacity = self.blocks.len() * self.block_size + self.block_size;
        let total_used = self.total_allocated;

        if total_capacity == 0 {
            0.0
        } else {
            1.0 - (total_used as f64 / total_capacity as f64)
        }
    }

    /// Reset arena (useful for batch processing)
    pub fn reset(&mut self) {
        self.blocks.clear();
        self.current_block.clear();
        self.current_block.resize(self.block_size, 0);
        self.current_pos = 0;
        self.total_allocated = 0;
        self.num_allocations = 0;
    }
}

/// Hash-consing table for structural sharing
///
/// Maintains a global table of canonicalized terms, ensuring that
/// structurally identical terms share the same memory representation.
/// This reduces memory usage and enables fast structural equality checking.
#[derive(Debug)]
pub struct HashConsTable {
    /// Table mapping hashes to canonicalized terms
    table: HashMap<u64, Arc<Term>>,
    /// Cache for recently accessed terms
    cache: HashMap<u64, Arc<Term>>,
    /// Maximum cache size
    max_cache_size: usize,
    /// Statistics
    hits: u64,
    misses: u64,
}

impl HashConsTable {
    /// Create a new hash-consing table
    pub fn new(initial_capacity: usize) -> Self {
        HashConsTable {
            table: HashMap::with_capacity(initial_capacity),
            cache: HashMap::new(),
            max_cache_size: initial_capacity / 4, // Cache is smaller than main table
            hits: 0,
            misses: 0,
        }
    }

    /// Hash-cons a term (get canonical representation)
    pub fn hash_cons(&mut self, term: Term) -> Arc<Term> {
        let hash = self.hash_term(&term);

        // Check cache first
        if let Some(cached) = self.cache.get(&hash) {
            self.hits += 1;
            record_metric(|metrics| metrics.hash_cons_hits += 1);
            return cached.clone();
        }

        // Check main table
        if let Some(existing) = self.table.get(&hash) {
            let result = existing.clone();
            self.hits += 1;
            record_metric(|metrics| metrics.hash_cons_hits += 1);

            // Add to cache for faster future access
            self.add_to_cache(hash, result.clone());
            return result;
        }

        // Term not found, create new canonical representation
        self.misses += 1;
        record_metric(|metrics| metrics.hash_cons_misses += 1);

        let canonical = Arc::new(term);
        self.table.insert(hash, canonical.clone());
        self.add_to_cache(hash, canonical.clone());

        canonical
    }

    /// Add term to cache with eviction if necessary
    fn add_to_cache(&mut self, hash: u64, term: Arc<Term>) {
        if self.cache.len() >= self.max_cache_size {
            // Simple eviction: remove oldest entry
            if let Some(&first_key) = self.cache.keys().next() {
                self.cache.remove(&first_key);
            }
        }
        self.cache.insert(hash, term);
    }

    /// Hash a term for hash-consing
    fn hash_term(&self, term: &Term) -> u64 {
        let mut hasher = DefaultHasher::new();
        term.hash(&mut hasher);
        hasher.finish()
    }

    /// Hash-cons all subterms recursively
    pub fn hash_cons_deep(&mut self, term: Term) -> Arc<Term> {
        let processed = match term {
            Term::Var(_) | Term::Universe(_) | Term::Meta(_) => term,

            Term::Pi(domain, codomain) => {
                let hc_domain = self.hash_cons_deep((*domain).clone());
                let hc_codomain = self.hash_cons_deep((*codomain).clone());
                Term::Pi(hc_domain, hc_codomain)
            },

            Term::Lambda(body) => {
                let hc_body = self.hash_cons_deep((*body).clone());
                Term::Lambda(hc_body)
            },

            Term::App(function, argument) => {
                let hc_function = self.hash_cons_deep((*function).clone());
                let hc_argument = self.hash_cons_deep((*argument).clone());
                Term::App(hc_function, hc_argument)
            },

            Term::Let(binding, body) => {
                let hc_binding = self.hash_cons_deep((*binding).clone());
                let hc_body = self.hash_cons_deep((*body).clone());
                Term::Let(hc_binding, hc_body)
            },
        };

        self.hash_cons(processed)
    }

    /// Get hash-consing statistics
    pub fn stats(&self) -> HashConsStats {
        HashConsStats {
            table_size: self.table.len(),
            cache_size: self.cache.len(),
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
        }
    }

    /// Clear the hash-consing table
    pub fn clear(&mut self) {
        self.table.clear();
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Compact the table by removing unused entries
    pub fn compact(&mut self) {
        // Remove entries with refcount == 1 (only held by table)
        self.table.retain(|_, term| Arc::strong_count(term) > 1);
        self.cache.retain(|_, term| Arc::strong_count(term) > 1);
    }
}

/// Memory pool for value allocation
///
/// Specialized allocator for Value objects with fast allocation
/// and bulk deallocation capabilities.
#[derive(Debug)]
pub struct ValuePool {
    /// Pre-allocated value slots
    pool: Vec<Option<Value>>,
    /// Next available slot
    next_free: usize,
    /// Free list for recycled slots
    free_list: Vec<usize>,
    /// Pool statistics
    total_allocations: u64,
    total_deallocations: u64,
}

impl ValuePool {
    /// Create a new value pool
    pub fn new(initial_capacity: usize) -> Self {
        ValuePool {
            pool: vec![None; initial_capacity],
            next_free: 0,
            free_list: Vec::new(),
            total_allocations: 0,
            total_deallocations: 0,
        }
    }

    /// Allocate a value from the pool
    pub fn allocate(&mut self, value: Value) -> usize {
        let slot = if let Some(free_slot) = self.free_list.pop() {
            free_slot
        } else if self.next_free < self.pool.len() {
            let slot = self.next_free;
            self.next_free += 1;
            slot
        } else {
            // Expand pool
            let slot = self.pool.len();
            self.pool.resize(self.pool.len() * 2, None);
            self.next_free = slot + 1;
            slot
        };

        self.pool[slot] = Some(value);
        self.total_allocations += 1;
        slot
    }

    /// Deallocate a value back to the pool
    pub fn deallocate(&mut self, slot: usize) {
        if slot < self.pool.len() && self.pool[slot].is_some() {
            self.pool[slot] = None;
            self.free_list.push(slot);
            self.total_deallocations += 1;
        }
    }

    /// Get a value from the pool
    pub fn get(&self, slot: usize) -> Option<&Value> {
        self.pool.get(slot).and_then(|opt| opt.as_ref())
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            capacity: self.pool.len(),
            used: self.next_free - self.free_list.len(),
            free_slots: self.free_list.len(),
            total_allocations: self.total_allocations,
            total_deallocations: self.total_deallocations,
        }
    }

    /// Reset the pool
    pub fn reset(&mut self) {
        self.pool.clear();
        self.free_list.clear();
        self.next_free = 0;
        self.total_allocations = 0;
        self.total_deallocations = 0;
    }
}

/// Arena allocation statistics
#[derive(Clone, Debug)]
pub struct ArenaStats {
    pub total_allocated: usize,
    pub num_allocations: usize,
    pub num_blocks: usize,
    pub block_size: usize,
    pub current_block_used: usize,
    pub fragmentation: f64,
}

/// Hash-consing statistics
#[derive(Clone, Debug)]
pub struct HashConsStats {
    pub table_size: usize,
    pub cache_size: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// Pool allocation statistics
#[derive(Clone, Debug)]
pub struct PoolStats {
    pub capacity: usize,
    pub used: usize,
    pub free_slots: usize,
    pub total_allocations: u64,
    pub total_deallocations: u64,
}

/// Global memory management context
static mut GLOBAL_ARENA: Option<TermArena> = None;
static mut GLOBAL_HASH_CONS: Option<HashConsTable> = None;
static mut GLOBAL_VALUE_POOL: Option<ValuePool> = None;

/// Initialize global memory management
pub fn init_memory() {
    let config = get_config();

    unsafe {
        GLOBAL_ARENA = Some(TermArena::new(config.arena_block_size));
        if config.enable_hash_consing {
            GLOBAL_HASH_CONS = Some(HashConsTable::new(config.hash_table_capacity));
        }
        GLOBAL_VALUE_POOL = Some(ValuePool::new(config.hash_table_capacity));
    }
}

/// Allocate a term in the global arena
pub fn arena_allocate<T>(value: T) -> &'static mut T {
    unsafe {
        if let Some(ref mut arena) = GLOBAL_ARENA {
            std::mem::transmute(arena.allocate(value))
        } else {
            panic!("Memory management not initialized")
        }
    }
}

/// Hash-cons a term using the global table
pub fn hash_cons_term(term: Term) -> Arc<Term> {
    unsafe {
        if let Some(ref mut table) = GLOBAL_HASH_CONS {
            table.hash_cons(term)
        } else {
            // Fallback: just wrap in Rc without hash-consing
            Arc::new(term)
        }
    }
}

/// Hash-cons a term deeply
pub fn hash_cons_deep(term: Term) -> Arc<Term> {
    unsafe {
        if let Some(ref mut table) = GLOBAL_HASH_CONS {
            table.hash_cons_deep(term)
        } else {
            Arc::new(term)
        }
    }
}

/// Get memory management statistics
pub fn get_memory_stats() -> MemoryStats {
    unsafe {
        let arena_stats = GLOBAL_ARENA.as_ref().map(|a| a.stats());
        let hash_cons_stats = GLOBAL_HASH_CONS.as_ref().map(|h| h.stats());
        let pool_stats = GLOBAL_VALUE_POOL.as_ref().map(|p| p.stats());

        MemoryStats {
            arena: arena_stats,
            hash_cons: hash_cons_stats,
            pool: pool_stats,
        }
    }
}

/// Compact memory (remove unused entries)
pub fn compact_memory() {
    unsafe {
        if let Some(ref mut table) = GLOBAL_HASH_CONS {
            table.compact();
        }
    }
}

/// Combined memory statistics
#[derive(Clone, Debug)]
pub struct MemoryStats {
    pub arena: Option<ArenaStats>,
    pub hash_cons: Option<HashConsStats>,
    pub pool: Option<PoolStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Term;

    #[test]
    fn test_arena_allocation() {
        let mut arena = TermArena::new(1024);

        // Allocate some values
        let val1 = arena.allocate(42u32);
        assert_eq!(*val1, 42);

        let val2 = arena.allocate(84u32);
        assert_eq!(*val2, 84);

        let stats = arena.stats();
        assert!(stats.total_allocated >= 8); // At least 2 u32s
        assert_eq!(stats.num_allocations, 2);
    }

    #[test]
    fn test_hash_consing() {
        let mut table = HashConsTable::new(64);

        let term1 = Term::universe(0);
        let term2 = Term::universe(0);

        let hc1 = table.hash_cons(term1);
        let hc2 = table.hash_cons(term2);

        // Should be the same canonical representation
        assert!(Arc::ptr_eq(&hc1, &hc2));

        let stats = table.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_value_pool() {
        let mut pool = ValuePool::new(10);

        let value = Value::universe(crate::core::Level(0));
        let slot = pool.allocate(value.clone());

        assert_eq!(pool.get(slot), Some(&value));

        pool.deallocate(slot);
        assert_eq!(pool.get(slot), None);

        let stats = pool.stats();
        assert_eq!(stats.total_allocations, 1);
        assert_eq!(stats.total_deallocations, 1);
    }

    #[test]
    fn test_deep_hash_consing() {
        let mut table = HashConsTable::new(64);

        // Create two identical complex terms
        let term1 = Term::lambda(Term::app(Term::var(0), Term::universe(0)));
        let term2 = Term::lambda(Term::app(Term::var(0), Term::universe(0)));

        let hc1 = table.hash_cons_deep(term1);
        let hc2 = table.hash_cons_deep(term2);

        // Root terms should be shared
        assert!(Arc::ptr_eq(&hc1, &hc2));
    }
}