//! TTT-Lean 4 Integration Layer
//!
//! Provides bidirectional translation and verification through Lean's kernel
//! with access to mathlib4's theorem database.

#[cfg(feature = "lean-integration")]
pub mod translation;
#[cfg(feature = "lean-integration")]
pub mod types;
#[cfg(feature = "lean-integration")]
pub mod context;
#[cfg(feature = "lean-integration")]
pub mod error;
#[cfg(feature = "lean-integration")]
pub mod optimize;
#[cfg(feature = "lean-integration")]
pub mod parallel;
#[cfg(feature = "lean-integration")]
pub mod mathlib;
#[cfg(feature = "lean-integration")]
pub mod verification;
#[cfg(feature = "lean-integration")]
pub mod cache;
#[cfg(feature = "lean-integration")]
pub mod server;

// Always include modules regardless of feature flag for build compatibility
#[cfg(not(feature = "lean-integration"))]
pub mod translation;
#[cfg(not(feature = "lean-integration"))]
pub mod types;
#[cfg(not(feature = "lean-integration"))]
pub mod context;
#[cfg(not(feature = "lean-integration"))]
pub mod error;

#[cfg(feature = "lean-integration")]
use std::sync::Arc;
#[cfg(feature = "lean-integration")]
use parking_lot::RwLock;
#[cfg(feature = "lean-integration")]
use dashmap::DashMap;
#[cfg(feature = "lean-integration")]
use once_cell::sync::Lazy;

#[cfg(feature = "lean-integration")]
pub use translation::{LeanTranslator, TranslationStatistics};
#[cfg(feature = "lean-integration")]
pub use types::{LeanTerm, LeanLevel, LeanName};
#[cfg(feature = "lean-integration")]
pub use context::TranslationContext;
#[cfg(feature = "lean-integration")]
pub use error::{LeanError, Result};
#[cfg(feature = "lean-integration")]
pub use optimize::{OptimizedTranslator, CacheStatistics, ContextPool};
#[cfg(feature = "lean-integration")]
pub use parallel::{ParallelTranslator, TranslationTask, TaskPriority, ParallelPerformanceStats};
#[cfg(feature = "lean-integration")]
pub use mathlib::{TheoremDatabase, MathlibTheorem, SearchQuery, TheoremCategory};
#[cfg(feature = "lean-integration")]
pub use verification::{LeanVerifier, ProofResult, VerificationStrategy, VerificationConfig};
#[cfg(feature = "lean-integration")]
pub use cache::{CacheManager, TranslationCache, VerificationCache, TheoremCache};
#[cfg(feature = "lean-integration")]
pub use server::{LeanServer, LeanServerConfig, ServerInfo};

#[cfg(feature = "lean-integration")]
use crate::core::{Term, Level};

/// Global Lean environment (initialized once)
#[cfg(feature = "lean-integration")]
static LEAN_ENV: Lazy<Arc<RwLock<LeanEnvironment>>> = Lazy::new(|| {
    Arc::new(RwLock::new(
        LeanEnvironment::new()
            .expect("Failed to initialize Lean environment")
    ))
});

/// Simplified Lean environment for initial implementation
#[cfg(feature = "lean-integration")]
#[derive(Debug)]
pub struct LeanEnvironment {
    /// Counter for fresh variable generation
    fresh_counter: u64,
}

#[cfg(feature = "lean-integration")]
impl LeanEnvironment {
    /// Create a new Lean environment
    pub fn new() -> Result<Self> {
        tracing::info!("Initializing TTT-Lean environment");
        Ok(Self {
            fresh_counter: 0,
        })
    }

    /// Generate a fresh variable name
    pub fn fresh_name(&mut self, hint: &str) -> String {
        self.fresh_counter += 1;
        format!("{}_{}", hint, self.fresh_counter)
    }
}

/// Central bridge coordinating all Lean operations with mathlib integration
#[cfg(feature = "lean-integration")]
pub struct LeanBridge {
    /// Core translation functionality
    translator: Arc<LeanTranslator>,

    /// Lean server for kernel operations
    server: Arc<LeanServer>,

    /// Mathlib theorem database
    theorem_db: Arc<TheoremDatabase>,

    /// Proof verification engine
    verifier: Arc<LeanVerifier>,

    /// Multi-tiered caching system
    cache_manager: Arc<CacheManager>,

    /// Legacy term cache for backward compatibility
    term_cache: Arc<DashMap<Term, LeanTerm>>,

    /// Performance metrics
    metrics: Arc<Metrics>,

    /// Bridge configuration
    config: LeanBridgeConfig,
}

/// Configuration for the Lean bridge
#[cfg(feature = "lean-integration")]
#[derive(Debug, Clone)]
pub struct LeanBridgeConfig {
    /// Path to mathlib installation
    pub mathlib_path: std::path::PathBuf,

    /// Cache directory
    pub cache_dir: std::path::PathBuf,

    /// Server configuration
    pub server_config: LeanServerConfig,

    /// Verification configuration
    pub verification_config: VerificationConfig,

    /// Whether to preload common theorems
    pub preload_theorems: bool,

    /// Maximum bridge initialization timeout
    pub init_timeout: std::time::Duration,
}

impl Default for LeanBridgeConfig {
    fn default() -> Self {
        Self {
            mathlib_path: std::path::PathBuf::from("mathlib4"),
            cache_dir: std::path::PathBuf::from("cache"),
            server_config: LeanServerConfig::default(),
            verification_config: VerificationConfig::default(),
            preload_theorems: true,
            init_timeout: std::time::Duration::from_secs(60),
        }
    }
}

#[cfg(feature = "lean-integration")]
impl LeanBridge {
    /// Initialize the bridge with full mathlib integration
    pub async fn new() -> Result<Self> {
        Self::with_config(LeanBridgeConfig::default()).await
    }

    /// Initialize the bridge with custom configuration
    pub async fn with_config(config: LeanBridgeConfig) -> Result<Self> {
        tracing::info!("Initializing TTT-Lean bridge with mathlib integration");

        // Initialize cache manager first
        let cache_config = cache::CacheManagerConfig {
            base_dir: config.cache_dir.clone(),
            warm_on_startup: config.preload_theorems,
            cleanup_interval: std::time::Duration::from_secs(3600),
        };
        let cache_manager = Arc::new(
            CacheManager::new(cache_config).await
                .map_err(|e| LeanError::Internal(format!("Cache initialization failed: {}", e)))?
        );

        // Initialize Lean server
        let server = Arc::new(
            LeanServer::new(config.server_config.clone()).await
                .map_err(|e| LeanError::Internal(format!("Server initialization failed: {}", e)))?
        );

        // Load mathlib theorem database
        let theorem_db = Arc::new(
            TheoremDatabase::load_from_mathlib(config.mathlib_path.clone()).await
                .map_err(|e| LeanError::Internal(format!("Mathlib loading failed: {}", e)))?
        );

        // Initialize verification engine
        let verifier = Arc::new(
            LeanVerifier::new(
                server.clone(),
                theorem_db.clone(),
                Arc::new(cache_manager.verification.clone()),
                config.verification_config.clone(),
            ).await
            .map_err(|e| LeanError::Internal(format!("Verifier initialization failed: {}", e)))?
        );

        let bridge = Self {
            translator: Arc::new(LeanTranslator::new()),
            server,
            theorem_db,
            verifier,
            cache_manager,
            term_cache: Arc::new(DashMap::new()),
            metrics: Arc::new(Metrics::new()),
            config,
        };

        // Preload common theorems if configured
        if bridge.config.preload_theorems {
            bridge.preload_common_theorems().await?;
        }

        tracing::info!("TTT-Lean bridge initialized successfully");
        Ok(bridge)
    }

    /// Translate a TTT term to Lean with caching
    pub async fn translate_to_lean(&self, term: &Term) -> Result<LeanTerm> {
        self.metrics.record_translation_start();

        // Check cache first (legacy cache for now)
        if let Some(cached) = self.term_cache.get(term) {
            self.metrics.record_cache_hit();
            return Ok(cached.clone());
        }

        // Check persistent cache
        let cache_key = cache::CacheKey::Translation(term.clone());
        if let Some(cached) = self.cache_manager.translation.get(&cache_key).await {
            self.metrics.record_cache_hit();
            // Update legacy cache for fast access
            self.term_cache.insert(term.clone(), cached.clone());
            return Ok(cached);
        }

        // Translate to Lean
        let lean_term = self.translator.to_lean(term)?;

        // Cache translation in both caches
        self.term_cache.insert(term.clone(), lean_term.clone());
        let _ = self.cache_manager.translation.put(cache_key, lean_term.clone()).await;

        self.metrics.record_translation_complete();
        Ok(lean_term)
    }

    /// Translate a Lean term back to TTT
    pub fn translate_from_lean(&self, lean_term: &LeanTerm) -> Result<Term> {
        self.translator.from_lean(lean_term)
    }

    /// Verify a proof using the integrated verification engine
    pub async fn verify_proof(&self, goal: LeanTerm, proof_hint: Option<LeanTerm>) -> Result<ProofResult> {
        self.verifier.verify(goal, proof_hint).await
            .map_err(|e| LeanError::Internal(format!("Verification failed: {}", e)))
    }

    /// Attempt automated proof using mathlib theorems
    pub async fn auto_prove(&self, goal: LeanTerm) -> Result<ProofResult> {
        self.verifier.attempt_auto_proof(goal).await
            .map_err(|e| LeanError::Internal(format!("Auto proof failed: {}", e)))
    }

    /// Search for applicable theorems
    pub async fn search_theorems(&self, query: SearchQuery) -> Vec<MathlibTheorem> {
        self.theorem_db.search(&query)
    }

    /// Find theorems applicable to a specific goal
    pub async fn find_applicable_theorems(&self, goal: &LeanTerm) -> Vec<MathlibTheorem> {
        self.theorem_db.find_applicable(goal)
    }

    /// Type check a term using the Lean server
    pub async fn type_check(&self, term: LeanTerm) -> Result<LeanTerm> {
        self.server.type_check(term).await
            .map_err(|e| LeanError::Internal(format!("Type check failed: {}", e)))
    }

    /// Normalize a term using the Lean server
    pub async fn normalize(&self, term: LeanTerm) -> Result<LeanTerm> {
        self.server.normalize(term).await
            .map_err(|e| LeanError::Internal(format!("Normalization failed: {}", e)))
    }

    /// Apply a tactic to a goal
    pub async fn apply_tactic(&self, goal: LeanTerm, tactic: String) -> Result<LeanTerm> {
        self.server.apply_tactic(goal, tactic).await
            .map_err(|e| LeanError::Internal(format!("Tactic application failed: {}", e)))
    }

    /// Get comprehensive bridge statistics
    pub fn get_comprehensive_stats(&self) -> BridgeStats {
        BridgeStats {
            translation_metrics: BridgeTranslationMetrics {
                total_translations: self.metrics.translation_count(),
                cache_hit_rate: self.metrics.cache_hit_rate(),
            },
            server_metrics: ServerMetrics {
                success_rate: self.server.metrics().success_rate(),
                avg_response_time_ms: self.server.metrics().avg_response_time_ms.load(std::sync::atomic::Ordering::Relaxed),
                uptime: self.server.metrics().uptime(),
            },
            verification_metrics: VerificationMetrics {
                success_rate: self.verifier.metrics().success_rate(),
                cache_hit_rate: self.verifier.metrics().cache_hit_rate(),
            },
            theorem_db_stats: self.theorem_db.stats(),
            cache_stats: self.cache_manager.global_stats(),
        }
    }

    /// Preload common theorems for better performance
    async fn preload_common_theorems(&self) -> Result<()> {
        tracing::info!("Preloading common theorems");

        // Search for commonly used theorem categories
        let common_categories = vec![
            TheoremCategory::Logic,
            TheoremCategory::SetTheory,
            TheoremCategory::GroupTheory,
            TheoremCategory::RingTheory,
        ];

        for category in common_categories {
            let theorems = self.theorem_db.search(&SearchQuery::ByCategory(category));
            tracing::debug!("Preloaded {} theorems from {:?}", theorems.len(), category);
        }

        Ok(())
    }

    /// Get bridge metrics (legacy compatibility)
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Get theorem database
    pub fn theorem_database(&self) -> &TheoremDatabase {
        &self.theorem_db
    }

    /// Get verification engine
    pub fn verifier(&self) -> &LeanVerifier {
        &self.verifier
    }

    /// Get cache manager
    pub fn cache_manager(&self) -> &CacheManager {
        &self.cache_manager
    }

    /// Get server instance
    pub fn server(&self) -> &LeanServer {
        &self.server
    }

    /// Shutdown the bridge gracefully
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down TTT-Lean bridge");

        // Stop the server
        if let Err(e) = self.server.stop().await {
            tracing::warn!("Error stopping server: {}", e);
        }

        // Clear caches if needed
        if let Err(e) = self.cache_manager.clear_all().await {
            tracing::warn!("Error clearing caches: {}", e);
        }

        tracing::info!("TTT-Lean bridge shutdown complete");
        Ok(())
    }
}

/// Comprehensive bridge statistics
#[cfg(feature = "lean-integration")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BridgeStats {
    pub translation_metrics: BridgeTranslationMetrics,
    pub server_metrics: ServerMetrics,
    pub verification_metrics: VerificationMetrics,
    pub theorem_db_stats: mathlib::DatabaseStats,
    pub cache_stats: cache::GlobalCacheStats,
}

#[cfg(feature = "lean-integration")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BridgeTranslationMetrics {
    pub total_translations: u64,
    pub cache_hit_rate: f64,
}

#[cfg(feature = "lean-integration")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ServerMetrics {
    pub success_rate: f64,
    pub avg_response_time_ms: u64,
    pub uptime: Option<std::time::Duration>,
}

#[cfg(feature = "lean-integration")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VerificationMetrics {
    pub success_rate: f64,
    pub cache_hit_rate: f64,
}

#[cfg(feature = "lean-integration")]
#[derive(Debug)]
pub struct Metrics {
    translations: std::sync::atomic::AtomicU64,
    cache_hits: std::sync::atomic::AtomicU64,
    total_time_ms: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "lean-integration")]
impl Metrics {
    pub fn new() -> Self {
        Self {
            translations: std::sync::atomic::AtomicU64::new(0),
            cache_hits: std::sync::atomic::AtomicU64::new(0),
            total_time_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn record_translation_start(&self) {
        self.translations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_translation_complete(&self) {
        // In real implementation, would measure actual time
    }

    pub fn translation_count(&self) -> u64 {
        self.translations.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(std::sync::atomic::Ordering::Relaxed);
        let total = self.translations.load(std::sync::atomic::Ordering::Relaxed);
        if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        }
    }
}

// Re-export for non-lean builds (stub implementations)
#[cfg(not(feature = "lean-integration"))]
pub mod stub {
    use crate::core::Term;

    #[derive(Debug, Clone)]
    pub struct LeanTerm;

    #[derive(Debug)]
    pub struct LeanBridge;

    impl LeanBridge {
        pub fn new() -> Result<Self, String> {
            Err("Lean integration not enabled".to_string())
        }

        pub fn translate_to_lean(&self, _term: &Term) -> Result<LeanTerm, String> {
            Err("Lean integration not enabled".to_string())
        }
    }
}

#[cfg(not(feature = "lean-integration"))]
pub use stub::*;