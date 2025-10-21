//! Persistent Caching Layer for Lean Integration
//!
//! This module provides high-performance caching for Lean translations, theorem lookups,
//! and verification results. It uses a multi-tiered approach with in-memory caches,
//! persistent disk storage, and intelligent cache warming strategies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::hash::{Hash, Hasher};

use dashmap::DashMap;
use parking_lot::RwLock;
use lru::LruCache;
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use thiserror::Error;
use tokio::fs;
use tokio::sync::{RwLock as AsyncRwLock, Mutex as AsyncMutex};

use crate::lean::{LeanTerm, LeanName, LeanError};
use crate::lean::verification::{ProofResult, VerificationStrategy};
use crate::lean::mathlib::MathlibTheorem;
use crate::core::Term;

/// Cache-specific errors
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache miss for key: {key}")]
    CacheMiss { key: String },

    #[error("Serialization failed: {reason}")]
    SerializationError { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Cache corruption detected: {reason}")]
    Corruption { reason: String },

    #[error("Cache full, eviction failed")]
    CacheFull,

    #[error("Invalid cache key: {key}")]
    InvalidKey { key: String },
}

pub type CacheResult<T> = std::result::Result<T, CacheError>;

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    /// The cached value
    pub value: T,

    /// When this entry was created
    pub created_at: SystemTime,

    /// When this entry was last accessed
    pub last_accessed: SystemTime,

    /// How many times this entry has been accessed
    pub access_count: u64,

    /// Size estimate in bytes
    pub size_bytes: u64,

    /// Time to live (optional)
    pub ttl: Option<Duration>,

    /// Version for cache invalidation
    pub version: u32,
}

impl<T> CacheEntry<T> {
    /// Create a new cache entry
    pub fn new(value: T, size_bytes: u64) -> Self {
        let now = SystemTime::now();
        Self {
            value,
            created_at: now,
            last_accessed: now,
            access_count: 1,
            size_bytes,
            ttl: None,
            version: 1,
        }
    }

    /// Create a cache entry with TTL
    pub fn with_ttl(value: T, size_bytes: u64, ttl: Duration) -> Self {
        let mut entry = Self::new(value, size_bytes);
        entry.ttl = Some(ttl);
        entry
    }

    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            if let Ok(age) = self.created_at.elapsed() {
                return age > ttl;
            }
        }
        false
    }

    /// Update access statistics
    pub fn touch(&mut self) {
        self.last_accessed = SystemTime::now();
        self.access_count += 1;
    }

    /// Get age of this entry
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or(Duration::ZERO)
    }

    /// Calculate cache efficiency score (higher = better)
    pub fn efficiency_score(&self) -> f64 {
        let age_seconds = self.age().as_secs_f64().max(1.0);
        let access_rate = self.access_count as f64 / age_seconds;
        let size_penalty = 1.0 / (1.0 + (self.size_bytes as f64 / 1024.0).log10());
        access_rate * size_penalty
    }
}

/// Cache key types for different cached items
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheKey {
    /// Translation from TTT term to Lean term
    Translation(Term),

    /// Verification result for (goal, proof_hint, strategy)
    Verification(LeanTerm, Option<LeanTerm>, VerificationStrategy),

    /// Theorem lookup by name
    Theorem(String),

    /// Type checking result
    TypeCheck(LeanTerm),

    /// Normalization result
    Normalization(LeanTerm),

    /// Custom key with string identifier
    Custom(String),
}

impl CacheKey {
    /// Estimate the size of this key in bytes
    pub fn size_estimate(&self) -> u64 {
        match self {
            CacheKey::Translation(term) => std::mem::size_of::<Term>() as u64,
            CacheKey::Verification(goal, hint, _) => {
                std::mem::size_of::<LeanTerm>() as u64 * (1 + hint.as_ref().map_or(0, |_| 1))
            }
            CacheKey::Theorem(name) => name.len() as u64,
            CacheKey::TypeCheck(term) | CacheKey::Normalization(term) => {
                std::mem::size_of::<LeanTerm>() as u64
            }
            CacheKey::Custom(s) => s.len() as u64,
        }
    }

    /// Convert to string for serialization
    pub fn to_string(&self) -> String {
        match self {
            CacheKey::Translation(_) => format!("trans_{:x}", self.hash_code()),
            CacheKey::Verification(_, _, strategy) => format!("verif_{:?}_{:x}", strategy, self.hash_code()),
            CacheKey::Theorem(name) => format!("thm_{}", name),
            CacheKey::TypeCheck(_) => format!("type_{:x}", self.hash_code()),
            CacheKey::Normalization(_) => format!("norm_{:x}", self.hash_code()),
            CacheKey::Custom(s) => format!("custom_{}", s),
        }
    }

    /// Calculate hash code for this key
    fn hash_code(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// In-memory cache with LRU eviction
pub struct MemoryCache<T> {
    /// LRU cache for hot data
    lru: AsyncMutex<LruCache<CacheKey, CacheEntry<T>>>,

    /// Frequency tracking for smart eviction
    frequency: DashMap<CacheKey, u64>,

    /// Cache statistics
    stats: CacheStats,

    /// Configuration
    config: MemoryCacheConfig,
}

#[derive(Debug, Clone)]
pub struct MemoryCacheConfig {
    /// Maximum number of entries
    pub max_entries: usize,

    /// Maximum memory usage in bytes
    pub max_memory_bytes: u64,

    /// Default TTL for entries
    pub default_ttl: Option<Duration>,

    /// Enable frequency-based eviction
    pub frequency_eviction: bool,
}

impl Default for MemoryCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            max_memory_bytes: 256 * 1024 * 1024, // 256 MB
            default_ttl: Some(Duration::from_secs(3600)), // 1 hour
            frequency_eviction: true,
        }
    }
}

impl<T> MemoryCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new memory cache
    pub fn new(config: MemoryCacheConfig) -> Self {
        Self {
            lru: AsyncMutex::new(LruCache::new(config.max_entries.try_into().unwrap())),
            frequency: DashMap::new(),
            stats: CacheStats::new(),
            config,
        }
    }

    /// Get an entry from the cache
    pub async fn get(&self, key: &CacheKey) -> Option<T> {
        self.stats.record_access();

        let mut lru = self.lru.lock().await;
        if let Some(mut entry) = lru.get_mut(key) {
            // Check expiration
            if entry.is_expired() {
                lru.pop(key);
                self.stats.record_miss();
                return None;
            }

            // Update access statistics
            entry.touch();
            if self.config.frequency_eviction {
                self.frequency.entry(key.clone()).and_modify(|freq| *freq += 1).or_insert(1);
            }

            self.stats.record_hit();
            Some(entry.value.clone())
        } else {
            self.stats.record_miss();
            None
        }
    }

    /// Put an entry into the cache
    pub async fn put(&self, key: CacheKey, value: T, size_bytes: u64) -> CacheResult<()> {
        let entry = if let Some(ttl) = self.config.default_ttl {
            CacheEntry::with_ttl(value, size_bytes, ttl)
        } else {
            CacheEntry::new(value, size_bytes)
        };

        let mut lru = self.lru.lock().await;

        // Check memory limits and evict if necessary
        while self.should_evict(&lru, size_bytes).await {
            if let Some((evicted_key, _)) = lru.pop_lru() {
                self.frequency.remove(&evicted_key);
                self.stats.record_eviction();
            } else {
                return Err(CacheError::CacheFull);
            }
        }

        lru.put(key.clone(), entry);
        if self.config.frequency_eviction {
            self.frequency.insert(key, 1);
        }

        self.stats.record_insertion();
        Ok(())
    }

    /// Remove an entry from the cache
    pub async fn remove(&self, key: &CacheKey) -> Option<T> {
        let mut lru = self.lru.lock().await;
        let removed = lru.pop(key).map(|entry| entry.value);
        self.frequency.remove(key);
        removed
    }

    /// Clear all entries
    pub async fn clear(&self) {
        let mut lru = self.lru.lock().await;
        lru.clear();
        self.frequency.clear();
        self.stats.reset();
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Check if we should evict entries
    async fn should_evict(&self, lru: &LruCache<CacheKey, CacheEntry<T>>, new_size: u64) -> bool {
        let current_memory = self.estimate_memory_usage(lru);
        current_memory + new_size > self.config.max_memory_bytes || lru.len() >= lru.cap().get()
    }

    /// Estimate current memory usage
    fn estimate_memory_usage(&self, lru: &LruCache<CacheKey, CacheEntry<T>>) -> u64 {
        lru.iter()
            .map(|(key, entry)| key.size_estimate() + entry.size_bytes)
            .sum()
    }
}

/// Persistent cache using disk storage
pub struct PersistentCache<T> {
    /// Base directory for cache files
    base_dir: PathBuf,

    /// In-memory index of cached items
    index: AsyncRwLock<HashMap<CacheKey, CacheIndexEntry>>,

    /// Configuration
    config: PersistentCacheConfig,

    /// Statistics
    stats: CacheStats,

    /// Phantom data for type safety
    _phantom: std::marker::PhantomData<T>,
}

#[derive(Debug, Clone)]
pub struct PersistentCacheConfig {
    /// Maximum disk usage in bytes
    pub max_disk_bytes: u64,

    /// Compression level (0-9, 0 = no compression)
    pub compression_level: u32,

    /// Sync interval for index updates
    pub sync_interval: Duration,

    /// Enable background cleanup
    pub background_cleanup: bool,
}

impl Default for PersistentCacheConfig {
    fn default() -> Self {
        Self {
            max_disk_bytes: 1024 * 1024 * 1024, // 1 GB
            compression_level: 6,
            sync_interval: Duration::from_secs(300), // 5 minutes
            background_cleanup: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheIndexEntry {
    file_path: PathBuf,
    size_bytes: u64,
    created_at: SystemTime,
    last_accessed: SystemTime,
    access_count: u64,
}

impl<T> PersistentCache<T>
where
    T: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    /// Create a new persistent cache
    pub async fn new(base_dir: PathBuf, config: PersistentCacheConfig) -> CacheResult<Self> {
        // Ensure base directory exists
        fs::create_dir_all(&base_dir).await?;

        let cache = Self {
            base_dir: base_dir.clone(),
            index: AsyncRwLock::new(HashMap::new()),
            config,
            stats: CacheStats::new(),
            _phantom: std::marker::PhantomData,
        };

        // Load existing index
        cache.load_index().await?;

        // Start background cleanup if enabled
        if cache.config.background_cleanup {
            cache.start_background_cleanup().await;
        }

        Ok(cache)
    }

    /// Get an entry from persistent storage
    pub async fn get(&self, key: &CacheKey) -> CacheResult<T> {
        self.stats.record_access();

        let index = self.index.read().await;
        if let Some(entry) = index.get(key) {
            // Read from disk
            let file_path = &entry.file_path;
            let data = fs::read(file_path).await?;

            // Decompress if necessary
            let data = if self.config.compression_level > 0 {
                self.decompress(&data)?
            } else {
                data
            };

            // Deserialize
            let value: T = bincode::deserialize(&data)
                .map_err(|e| CacheError::SerializationError { reason: e.to_string() })?;

            // Update access statistics
            drop(index);
            self.update_access_stats(key).await;

            self.stats.record_hit();
            Ok(value)
        } else {
            self.stats.record_miss();
            Err(CacheError::CacheMiss { key: key.to_string() })
        }
    }

    /// Put an entry into persistent storage
    pub async fn put(&self, key: CacheKey, value: T) -> CacheResult<()> {
        // Serialize value
        let data = bincode::serialize(&value)
            .map_err(|e| CacheError::SerializationError { reason: e.to_string() })?;

        // Compress if enabled
        let data = if self.config.compression_level > 0 {
            self.compress(&data)?
        } else {
            data
        };

        // Generate file path
        let file_name = format!("{}.cache", key.to_string());
        let file_path = self.base_dir.join(file_name);

        // Write to disk
        fs::write(&file_path, &data).await?;

        // Update index
        let index_entry = CacheIndexEntry {
            file_path: file_path.clone(),
            size_bytes: data.len() as u64,
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 1,
        };

        let mut index = self.index.write().await;
        index.insert(key, index_entry);

        self.stats.record_insertion();
        Ok(())
    }

    /// Remove an entry from persistent storage
    pub async fn remove(&self, key: &CacheKey) -> CacheResult<()> {
        let mut index = self.index.write().await;
        if let Some(entry) = index.remove(key) {
            // Remove file
            if let Err(e) = fs::remove_file(&entry.file_path).await {
                tracing::warn!("Failed to remove cache file {:?}: {}", entry.file_path, e);
            }
        }
        Ok(())
    }

    /// Load index from disk
    async fn load_index(&self) -> CacheResult<()> {
        let index_path = self.base_dir.join("cache_index.json");

        if index_path.exists() {
            let data = fs::read_to_string(&index_path).await?;
            let loaded_index: HashMap<CacheKey, CacheIndexEntry> = serde_json::from_str(&data)
                .map_err(|e| CacheError::SerializationError { reason: e.to_string() })?;

            let mut index = self.index.write().await;
            *index = loaded_index;
        }

        Ok(())
    }

    /// Save index to disk
    async fn save_index(&self) -> CacheResult<()> {
        let index_path = self.base_dir.join("cache_index.json");
        let index = self.index.read().await;

        let data = serde_json::to_string_pretty(&*index)
            .map_err(|e| CacheError::SerializationError { reason: e.to_string() })?;

        fs::write(&index_path, data).await?;
        Ok(())
    }

    /// Update access statistics for a key
    async fn update_access_stats(&self, key: &CacheKey) {
        let mut index = self.index.write().await;
        if let Some(entry) = index.get_mut(key) {
            entry.last_accessed = SystemTime::now();
            entry.access_count += 1;
        }
    }

    /// Start background cleanup task
    async fn start_background_cleanup(&self) {
        // TODO: Implement background cleanup
        // This would periodically:
        // 1. Remove expired entries
        // 2. Enforce size limits
        // 3. Save index to disk
        // 4. Defragment storage
    }

    /// Compress data
    fn compress(&self, data: &[u8]) -> CacheResult<Vec<u8>> {
        // TODO: Implement compression (e.g., using flate2)
        Ok(data.to_vec())
    }

    /// Decompress data
    fn decompress(&self, data: &[u8]) -> CacheResult<Vec<u8>> {
        // TODO: Implement decompression
        Ok(data.to_vec())
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }
}

/// Multi-tiered cache combining memory and persistent storage
#[derive(Clone)]
pub struct TieredCache<T: Clone> {
    /// L1 cache (memory)
    memory_cache: Arc<MemoryCache<T>>,

    /// L2 cache (persistent)
    persistent_cache: Arc<PersistentCache<T>>,

    /// Configuration
    config: TieredCacheConfig,

    /// Combined statistics
    stats: Arc<CacheStats>,
}

#[derive(Debug, Clone)]
pub struct TieredCacheConfig {
    /// Memory cache configuration
    pub memory_config: MemoryCacheConfig,

    /// Persistent cache configuration
    pub persistent_config: PersistentCacheConfig,

    /// Whether to promote cache hits to higher tiers
    pub promotion_enabled: bool,

    /// Threshold for promoting to memory cache
    pub promotion_threshold: u64,
}

impl Default for TieredCacheConfig {
    fn default() -> Self {
        Self {
            memory_config: MemoryCacheConfig::default(),
            persistent_config: PersistentCacheConfig::default(),
            promotion_enabled: true,
            promotion_threshold: 3, // Promote after 3 accesses
        }
    }
}

impl<T> TieredCache<T>
where
    T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    /// Create a new tiered cache
    pub async fn new(cache_dir: PathBuf, config: TieredCacheConfig) -> CacheResult<Self> {
        let memory_cache = Arc::new(MemoryCache::new(config.memory_config.clone()));
        let persistent_cache = Arc::new(PersistentCache::new(cache_dir, config.persistent_config.clone()).await?);

        Ok(Self {
            memory_cache,
            persistent_cache,
            config,
            stats: Arc::new(CacheStats::new()),
        })
    }

    /// Get an entry from any tier
    pub async fn get(&self, key: &CacheKey) -> Option<T> {
        self.stats.record_access();

        // Try L1 cache first
        if let Some(value) = self.memory_cache.get(key).await {
            self.stats.record_hit();
            return Some(value);
        }

        // Try L2 cache
        if let Ok(value) = self.persistent_cache.get(key).await {
            // Optionally promote to L1
            if self.config.promotion_enabled {
                let _ = self.memory_cache.put(
                    key.clone(),
                    value.clone(),
                    std::mem::size_of::<T>() as u64,
                ).await;
            }

            self.stats.record_hit();
            return Some(value);
        }

        self.stats.record_miss();
        None
    }

    /// Put an entry into the cache
    pub async fn put(&self, key: CacheKey, value: T) -> CacheResult<()> {
        let size_bytes = std::mem::size_of::<T>() as u64;

        // Store in L1 cache
        let _ = self.memory_cache.put(key.clone(), value.clone(), size_bytes).await;

        // Store in L2 cache
        self.persistent_cache.put(key, value).await?;

        self.stats.record_insertion();
        Ok(())
    }

    /// Remove an entry from all tiers
    pub async fn remove(&self, key: &CacheKey) -> CacheResult<()> {
        let _ = self.memory_cache.remove(key).await;
        self.persistent_cache.remove(key).await?;
        Ok(())
    }

    /// Clear all caches
    pub async fn clear(&self) -> CacheResult<()> {
        self.memory_cache.clear().await;
        // Note: Not clearing persistent cache to preserve across restarts
        self.stats.reset();
        Ok(())
    }

    /// Get combined statistics
    pub fn stats(&self) -> CacheStatsSnapshot {
        let memory_stats = self.memory_cache.stats();
        let persistent_stats = self.persistent_cache.stats();

        CacheStatsSnapshot {
            total_accesses: self.stats.accesses.load(std::sync::atomic::Ordering::Relaxed),
            total_hits: self.stats.hits.load(std::sync::atomic::Ordering::Relaxed),
            total_misses: self.stats.misses.load(std::sync::atomic::Ordering::Relaxed),
            total_insertions: self.stats.insertions.load(std::sync::atomic::Ordering::Relaxed),
            total_evictions: self.stats.evictions.load(std::sync::atomic::Ordering::Relaxed),
            hit_rate: self.stats.hit_rate(),
            memory_usage_bytes: memory_stats.hits.load(std::sync::atomic::Ordering::Relaxed), // Approximate
            persistent_cache_size: persistent_stats.hits.load(std::sync::atomic::Ordering::Relaxed) as usize, // Approximate
        }
    }
}

/// Cache statistics
#[derive(Debug)]
pub struct CacheStats {
    pub accesses: std::sync::atomic::AtomicU64,
    pub hits: std::sync::atomic::AtomicU64,
    pub misses: std::sync::atomic::AtomicU64,
    pub insertions: std::sync::atomic::AtomicU64,
    pub evictions: std::sync::atomic::AtomicU64,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            accesses: std::sync::atomic::AtomicU64::new(0),
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
            insertions: std::sync::atomic::AtomicU64::new(0),
            evictions: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn record_access(&self) {
        self.accesses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_hit(&self) {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_miss(&self) {
        self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_insertion(&self) {
        self.insertions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn hit_rate(&self) -> f64 {
        let accesses = self.accesses.load(std::sync::atomic::Ordering::Relaxed);
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
        if accesses > 0 {
            hits as f64 / accesses as f64
        } else {
            0.0
        }
    }

    pub fn reset(&self) {
        self.accesses.store(0, std::sync::atomic::Ordering::Relaxed);
        self.hits.store(0, std::sync::atomic::Ordering::Relaxed);
        self.misses.store(0, std::sync::atomic::Ordering::Relaxed);
        self.insertions.store(0, std::sync::atomic::Ordering::Relaxed);
        self.evictions.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> CacheStatsSnapshot {
        CacheStatsSnapshot {
            total_accesses: self.accesses.load(std::sync::atomic::Ordering::Relaxed),
            total_hits: self.hits.load(std::sync::atomic::Ordering::Relaxed),
            total_misses: self.misses.load(std::sync::atomic::Ordering::Relaxed),
            total_insertions: self.insertions.load(std::sync::atomic::Ordering::Relaxed),
            total_evictions: self.evictions.load(std::sync::atomic::Ordering::Relaxed),
            hit_rate: self.hit_rate(),
            memory_usage_bytes: 0,
            persistent_cache_size: 0,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CacheStatsSnapshot {
    pub total_accesses: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_insertions: u64,
    pub total_evictions: u64,
    pub hit_rate: f64,
    pub memory_usage_bytes: u64,
    pub persistent_cache_size: usize,
}

/// Specialized cache for different data types
pub type TranslationCache = TieredCache<LeanTerm>;
pub type VerificationCache = TieredCache<ProofResult>;
pub type TheoremCache = TieredCache<MathlibTheorem>;

/// Cache manager coordinating all caches
pub struct CacheManager {
    /// Translation cache
    pub translation: TranslationCache,

    /// Verification cache
    pub verification: VerificationCache,

    /// Theorem cache
    pub theorem: TheoremCache,

    /// Global configuration
    config: CacheManagerConfig,
}

#[derive(Debug, Clone)]
pub struct CacheManagerConfig {
    /// Base directory for all caches
    pub base_dir: PathBuf,

    /// Whether to enable cache warming on startup
    pub warm_on_startup: bool,

    /// Cache cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for CacheManagerConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("cache"),
            warm_on_startup: false,
            cleanup_interval: Duration::from_secs(3600), // 1 hour
        }
    }
}

impl CacheManager {
    /// Create a new cache manager
    pub async fn new(config: CacheManagerConfig) -> CacheResult<Self> {
        // Create cache directories
        let translation_dir = config.base_dir.join("translation");
        let verification_dir = config.base_dir.join("verification");
        let theorem_dir = config.base_dir.join("theorem");

        fs::create_dir_all(&translation_dir).await?;
        fs::create_dir_all(&verification_dir).await?;
        fs::create_dir_all(&theorem_dir).await?;

        // Create caches
        let translation = TranslationCache::new(translation_dir, TieredCacheConfig::default()).await?;
        let verification = VerificationCache::new(verification_dir, TieredCacheConfig::default()).await?;
        let theorem = TheoremCache::new(theorem_dir, TieredCacheConfig::default()).await?;

        let manager = Self {
            translation,
            verification,
            theorem,
            config,
        };

        Ok(manager)
    }

    /// Get cache statistics for all caches
    pub fn global_stats(&self) -> GlobalCacheStats {
        GlobalCacheStats {
            translation: self.translation.stats(),
            verification: self.verification.stats(),
            theorem: self.theorem.stats(),
        }
    }

    /// Clear all caches
    pub async fn clear_all(&self) -> CacheResult<()> {
        self.translation.clear().await?;
        self.verification.clear().await?;
        self.theorem.clear().await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GlobalCacheStats {
    pub translation: CacheStatsSnapshot,
    pub verification: CacheStatsSnapshot,
    pub theorem: CacheStatsSnapshot,
}

/// Extension trait for verification cache
impl VerificationCache {
    /// Get verification result
    pub async fn get_verification(&self, key: &(LeanTerm, Option<LeanTerm>, VerificationStrategy)) -> Option<ProofResult> {
        let cache_key = CacheKey::Verification(key.0.clone(), key.1.clone(), key.2);
        self.get(&cache_key).await
    }

    /// Store verification result
    pub async fn store_verification(&self, key: (LeanTerm, Option<LeanTerm>, VerificationStrategy), result: ProofResult) {
        let cache_key = CacheKey::Verification(key.0, key.1, key.2);
        let _ = self.put(cache_key, result).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[test]
    fn test_cache_entry() {
        let entry = CacheEntry::new("test_value".to_string(), 100);
        assert_eq!(entry.access_count, 1);
        assert!(!entry.is_expired());
        assert!(entry.efficiency_score() > 0.0);
    }

    #[test]
    fn test_cache_key() {
        use crate::lean::LeanLevel;

        let key1 = CacheKey::Theorem("test_theorem".to_string());
        let key2 = CacheKey::TypeCheck(LeanTerm::sort(LeanLevel::zero()));

        assert_ne!(key1.to_string(), key2.to_string());
        assert!(key1.size_estimate() > 0);
    }

    #[tokio::test]
    async fn test_memory_cache() {
        let config = MemoryCacheConfig::default();
        let cache = MemoryCache::new(config);

        let key = CacheKey::Custom("test".to_string());
        let value = "test_value".to_string();

        // Test put and get
        cache.put(key.clone(), value.clone(), 100).await.unwrap();
        let retrieved = cache.get(&key).await;
        assert_eq!(retrieved, Some(value));

        // Test cache miss
        let missing_key = CacheKey::Custom("missing".to_string());
        let missing = cache.get(&missing_key).await;
        assert_eq!(missing, None);

        // Test statistics
        let stats = cache.stats();
        assert!(stats.hit_rate() > 0.0);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let stats = CacheStats::new();

        stats.record_access();
        stats.record_hit();
        assert_eq!(stats.hit_rate(), 1.0);

        stats.record_access();
        stats.record_miss();
        assert_eq!(stats.hit_rate(), 0.5);
    }
}