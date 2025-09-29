//! Benchmarks for TTT-Lean mathlib integration
//!
//! These benchmarks measure performance of key operations in the mathlib
//! integration system, including translation, caching, theorem search,
//! and proof verification.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

#[cfg(feature = "lean-integration")]
mod mathlib_benchmarks {
    use super::*;
    use ttt::lean::{
        LeanBridge, LeanBridgeConfig, LeanTerm, LeanLevel, LeanName,
        TheoremDatabase, SearchQuery, TheoremCategory,
        CacheManager, TranslationCache, VerificationCache,
        cache::{CacheKey, TieredCache, TieredCacheConfig},
    };
    use ttt::core::Term;
    use std::path::PathBuf;
    use tokio::runtime::Runtime;

    /// Setup function for benchmarks that need a Lean bridge
    fn setup_bridge() -> Option<LeanBridge> {
        let rt = Runtime::new().unwrap();

        let config = LeanBridgeConfig {
            mathlib_path: PathBuf::from("mathlib4"), // Assumes mathlib4 is available
            cache_dir: PathBuf::from("bench_cache"),
            preload_theorems: false, // Don't preload for cleaner benchmarks
            init_timeout: Duration::from_secs(10),
            ..Default::default()
        };

        rt.block_on(async {
            LeanBridge::with_config(config).await.ok()
        })
    }

    /// Generate test terms of varying complexity
    fn generate_test_terms() -> Vec<Term> {
        vec![
            // Simple variable
            Term::Var("x".to_string()),

            // More complex terms would be added here based on TTT's Term structure
            // For now using simple examples
            Term::Var("complex_var_name_for_testing".to_string()),
        ]
    }

    /// Generate test Lean terms
    fn generate_lean_terms() -> Vec<LeanTerm> {
        vec![
            LeanTerm::var("x"),
            LeanTerm::const_("Nat"),
            LeanTerm::sort(LeanLevel::zero()),
            LeanTerm::lambda("x", LeanTerm::const_("Nat"), LeanTerm::var("x")),
            LeanTerm::pi("A", LeanTerm::sort(LeanLevel::zero()), LeanTerm::var("A")),
            LeanTerm::app(
                LeanTerm::lambda("x", LeanTerm::const_("Nat"), LeanTerm::var("x")),
                LeanTerm::const_("42")
            ),
        ]
    }

    pub fn bench_translation_performance(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        if let Some(bridge) = setup_bridge() {
            let test_terms = generate_test_terms();

            let mut group = c.benchmark_group("translation");
            group.sample_size(100);
            group.measurement_time(Duration::from_secs(30));

            for (i, term) in test_terms.iter().enumerate() {
                group.bench_with_input(
                    BenchmarkId::new("translate_to_lean", i),
                    term,
                    |b, term| {
                        b.to_async(&rt).iter(|| async {
                            black_box(bridge.translate_to_lean(black_box(term)).await)
                        });
                    },
                );
            }

            group.finish();
        } else {
            eprintln!("Skipping translation benchmarks - Lean bridge not available");
        }
    }

    pub fn bench_cache_performance(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        rt.block_on(async {
            let cache_config = TieredCacheConfig::default();
            let cache = TieredCache::new(
                PathBuf::from("bench_cache"),
                cache_config
            ).await.unwrap();

            let test_data = generate_lean_terms();

            let mut group = c.benchmark_group("cache");
            group.sample_size(1000);

            // Benchmark cache insertions
            group.bench_function("cache_insert", |b| {
                b.to_async(&rt).iter(|| async {
                    for (i, term) in test_data.iter().enumerate() {
                        let key = CacheKey::Custom(format!("bench_key_{}", i));
                        black_box(cache.put(black_box(key), black_box(term.clone())).await)
                    }
                });
            });

            // Populate cache for retrieval benchmarks
            for (i, term) in test_data.iter().enumerate() {
                let key = CacheKey::Custom(format!("bench_key_{}", i));
                let _ = cache.put(key, term.clone()).await;
            }

            // Benchmark cache retrievals
            group.bench_function("cache_retrieve", |b| {
                b.to_async(&rt).iter(|| async {
                    for i in 0..test_data.len() {
                        let key = CacheKey::Custom(format!("bench_key_{}", i));
                        black_box(cache.get(black_box(&key)).await)
                    }
                });
            });

            group.finish();
        });
    }

    pub fn bench_theorem_search(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        if let Some(bridge) = setup_bridge() {
            let mut group = c.benchmark_group("theorem_search");
            group.sample_size(50);
            group.measurement_time(Duration::from_secs(20));

            // Benchmark category-based search
            for category in &[
                TheoremCategory::Logic,
                TheoremCategory::GroupTheory,
                TheoremCategory::RealAnalysis,
                TheoremCategory::Topology,
            ] {
                group.bench_with_input(
                    BenchmarkId::new("search_by_category", format!("{:?}", category)),
                    category,
                    |b, &category| {
                        b.to_async(&rt).iter(|| async {
                            black_box(bridge.search_theorems(
                                black_box(SearchQuery::ByCategory(category))
                            ).await)
                        });
                    },
                );
            }

            // Benchmark name-based search
            let search_terms = vec!["add_comm", "mul_assoc", "zero_add", "succ"];
            for term in &search_terms {
                group.bench_with_input(
                    BenchmarkId::new("search_by_name", term),
                    term,
                    |b, &term| {
                        b.to_async(&rt).iter(|| async {
                            black_box(bridge.search_theorems(
                                black_box(SearchQuery::ByName(term.to_string()))
                            ).await)
                        });
                    },
                );
            }

            group.finish();
        } else {
            eprintln!("Skipping theorem search benchmarks - Lean bridge not available");
        }
    }

    pub fn bench_verification_strategies(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        if let Some(bridge) = setup_bridge() {
            let mut group = c.benchmark_group("verification");
            group.sample_size(20);
            group.measurement_time(Duration::from_secs(60));

            // Simple tautology for testing
            let tautology = LeanTerm::pi(
                "P",
                LeanTerm::sort(LeanLevel::zero()),
                LeanTerm::pi(
                    "_",
                    LeanTerm::var("P"),
                    LeanTerm::var("P")
                )
            );

            group.bench_function("verify_tautology", |b| {
                b.to_async(&rt).iter(|| async {
                    black_box(bridge.verify_proof(
                        black_box(tautology.clone()),
                        black_box(None)
                    ).await)
                });
            });

            group.bench_function("auto_prove_tautology", |b| {
                b.to_async(&rt).iter(|| async {
                    black_box(bridge.auto_prove(
                        black_box(tautology.clone())
                    ).await)
                });
            });

            group.finish();
        } else {
            eprintln!("Skipping verification benchmarks - Lean bridge not available");
        }
    }

    pub fn bench_concurrent_operations(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        if let Some(bridge) = setup_bridge() {
            let mut group = c.benchmark_group("concurrent");
            group.sample_size(10);
            group.measurement_time(Duration::from_secs(30));

            let test_terms = generate_test_terms();

            group.bench_function("concurrent_translation", |b| {
                b.to_async(&rt).iter(|| async {
                    let handles = test_terms.iter().map(|term| {
                        let bridge = &bridge;
                        let term = term.clone();
                        tokio::spawn(async move {
                            bridge.translate_to_lean(&term).await
                        })
                    }).collect::<Vec<_>>();

                    black_box(futures::future::join_all(handles).await)
                });
            });

            group.finish();
        } else {
            eprintln!("Skipping concurrent benchmarks - Lean bridge not available");
        }
    }

    pub fn bench_memory_usage(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        let mut group = c.benchmark_group("memory");
        group.sample_size(50);

        // Benchmark memory allocation for different term types
        group.bench_function("lean_term_creation", |b| {
            b.iter(|| {
                let terms = black_box(generate_lean_terms());
                drop(terms);
            });
        });

        // Benchmark cache memory usage
        group.bench_function("cache_memory_usage", |b| {
            b.to_async(&rt).iter(|| async {
                let cache = TieredCache::new(
                    PathBuf::from("temp_bench_cache"),
                    TieredCacheConfig::default()
                ).await.unwrap();

                for i in 0..1000 {
                    let key = CacheKey::Custom(format!("temp_key_{}", i));
                    let value = LeanTerm::const_(&format!("value_{}", i));
                    let _ = cache.put(key, value).await;
                }

                black_box(cache)
            });
        });

        group.finish();
    }

    criterion_group!(
        mathlib_benches,
        bench_translation_performance,
        bench_cache_performance,
        bench_theorem_search,
        bench_verification_strategies,
        bench_concurrent_operations,
        bench_memory_usage
    );
}

#[cfg(not(feature = "lean-integration"))]
mod stub_benchmarks {
    use super::*;

    pub fn bench_stub_operations(c: &mut Criterion) {
        let mut group = c.benchmark_group("stub");

        group.bench_function("stub_bridge_creation", |b| {
            b.iter(|| {
                // Benchmark the stub implementation
                black_box(ttt::lean::LeanBridge::new())
            });
        });

        group.finish();
    }

    criterion_group!(stub_benches, bench_stub_operations);
}

// Main benchmark registration
#[cfg(feature = "lean-integration")]
criterion_main!(mathlib_benchmarks::mathlib_benches);

#[cfg(not(feature = "lean-integration"))]
criterion_main!(stub_benchmarks::stub_benches);