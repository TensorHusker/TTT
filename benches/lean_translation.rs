//! Performance benchmarks for TTT-Lean bridge translation operations
//!
//! This module provides comprehensive performance benchmarks to measure
//! and monitor the efficiency of TTT-Lean bridge operations, including
//! translation speed, memory usage, cache performance, and scalability.

#![cfg(feature = "lean-integration")]

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use ttt::core::{Term, Level};
use ttt::lean::{LeanBridge, LeanTerm};
use std::time::Duration;

/// Benchmark suite entry point
criterion_group!(benches,
    bench_translation_to_lean,
    bench_translation_from_lean,
    bench_roundtrip_translation,
    bench_cache_performance,
    bench_term_complexity_scaling,
    bench_concurrent_translation,
    bench_memory_usage,
    bench_large_term_translation
);
criterion_main!(benches);

/// Benchmark TTT → Lean translation speed for various term types
fn bench_translation_to_lean(c: &mut Criterion) {
    let bridge = LeanBridge::new().expect("Failed to create bridge");

    let mut group = c.benchmark_group("ttt_to_lean_translation");

    // Simple terms
    group.bench_function("variable", |b| {
        let term = Term::var(0);
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("universe", |b| {
        let term = Term::universe(5);
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("simple_lambda", |b| {
        let term = Term::lambda(Term::var(0));
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("simple_pi", |b| {
        let term = Term::pi(Term::type_0(), Term::var(0));
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("simple_app", |b| {
        let term = Term::app(Term::var(1), Term::var(0));
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("let_binding", |b| {
        let term = Term::let_in(Term::var(1), Term::var(0));
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    // Complex terms
    group.bench_function("church_numeral_5", |b| {
        let term = create_church_numeral(5);
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("nested_lambda_10", |b| {
        let term = create_nested_lambda(10);
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.bench_function("complex_pi_type", |b| {
        let term = create_complex_pi_type();
        b.iter(|| bridge.translate_to_lean(black_box(&term)))
    });

    group.finish();
}

/// Benchmark Lean → TTT translation speed
fn bench_translation_from_lean(c: &mut Criterion) {
    let bridge = LeanBridge::new().expect("Failed to create bridge");

    let mut group = c.benchmark_group("lean_to_ttt_translation");

    // Prepare pre-translated Lean terms
    let simple_terms = vec![
        ("variable", bridge.translate_to_lean(&Term::var(0)).unwrap()),
        ("universe", bridge.translate_to_lean(&Term::universe(3)).unwrap()),
        ("lambda", bridge.translate_to_lean(&Term::lambda(Term::var(0))).unwrap()),
        ("pi", bridge.translate_to_lean(&Term::pi(Term::type_0(), Term::var(0))).unwrap()),
        ("app", bridge.translate_to_lean(&Term::app(Term::var(1), Term::var(0))).unwrap()),
    ];

    for (name, lean_term) in simple_terms {
        group.bench_function(name, |b| {
            b.iter(|| bridge.translate_from_lean(black_box(&lean_term)))
        });
    }

    // Complex terms
    let complex_terms = vec![
        ("church_3", bridge.translate_to_lean(&create_church_numeral(3)).unwrap()),
        ("nested_lambda_8", bridge.translate_to_lean(&create_nested_lambda(8)).unwrap()),
        ("complex_pi", bridge.translate_to_lean(&create_complex_pi_type()).unwrap()),
    ];

    for (name, lean_term) in complex_terms {
        group.bench_function(name, |b| {
            b.iter(|| bridge.translate_from_lean(black_box(&lean_term)))
        });
    }

    group.finish();
}

/// Benchmark roundtrip translation performance (TTT → Lean → TTT)
fn bench_roundtrip_translation(c: &mut Criterion) {
    let bridge = LeanBridge::new().expect("Failed to create bridge");

    let mut group = c.benchmark_group("roundtrip_translation");

    let test_terms = vec![
        ("simple_lambda", Term::lambda(Term::var(0))),
        ("church_numeral_3", create_church_numeral(3)),
        ("nested_app", create_nested_application(5)),
        ("complex_type", create_complex_pi_type()),
    ];

    for (name, term) in test_terms {
        group.bench_function(name, |b| {
            b.iter(|| {
                let lean_term = bridge.translate_to_lean(black_box(&term)).unwrap();
                bridge.translate_from_lean(black_box(&lean_term)).unwrap()
            })
        });
    }

    group.finish();
}

/// Benchmark cache performance and hit rates
fn bench_cache_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_performance");

    let terms = vec![
        create_church_numeral(2),
        create_nested_lambda(5),
        create_complex_pi_type(),
    ];

    // Cold cache (first access)
    group.bench_function("cold_cache", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = Duration::new(0, 0);

            for _ in 0..iters {
                // Fresh bridge for each iteration to ensure cold cache
                let bridge = LeanBridge::new().expect("Failed to create bridge");

                let start = std::time::Instant::now();
                for term in &terms {
                    let _ = bridge.translate_to_lean(black_box(term));
                }
                total_duration += start.elapsed();
            }

            total_duration
        })
    });

    // Warm cache (repeated access)
    group.bench_function("warm_cache", |b| {
        let bridge = LeanBridge::new().expect("Failed to create bridge");

        // Pre-warm the cache
        for term in &terms {
            let _ = bridge.translate_to_lean(term);
        }

        b.iter(|| {
            for term in &terms {
                let _ = bridge.translate_to_lean(black_box(term));
            }
        })
    });

    // Mixed workload (some cached, some not)
    group.bench_function("mixed_workload", |b| {
        let bridge = LeanBridge::new().expect("Failed to create bridge");

        // Cache half the terms
        for (i, term) in terms.iter().enumerate() {
            if i % 2 == 0 {
                let _ = bridge.translate_to_lean(term);
            }
        }

        b.iter(|| {
            for term in &terms {
                let _ = bridge.translate_to_lean(black_box(term));
            }
        })
    });

    group.finish();
}

/// Benchmark scaling behavior with term complexity
fn bench_term_complexity_scaling(c: &mut Criterion) {
    let bridge = LeanBridge::new().expect("Failed to create bridge");

    let mut group = c.benchmark_group("complexity_scaling");
    group.sample_size(50); // Reduce sample size for expensive benchmarks

    // Test scaling with lambda nesting depth
    for depth in [5, 10, 15, 20, 25].iter() {
        let term = create_nested_lambda(*depth);
        group.throughput(Throughput::Elements(*depth as u64));
        group.bench_with_input(
            BenchmarkId::new("nested_lambda", depth),
            depth,
            |b, _| {
                b.iter(|| bridge.translate_to_lean(black_box(&term)))
            }
        );
    }

    // Test scaling with Church numeral size
    for n in [2, 5, 8, 12, 15].iter() {
        let term = create_church_numeral(*n);
        group.throughput(Throughput::Elements(*n as u64));
        group.bench_with_input(
            BenchmarkId::new("church_numeral", n),
            n,
            |b, _| {
                b.iter(|| bridge.translate_to_lean(black_box(&term)))
            }
        );
    }

    // Test scaling with application chain length
    for length in [3, 6, 9, 12, 15].iter() {
        let term = create_nested_application(*length);
        group.throughput(Throughput::Elements(*length as u64));
        group.bench_with_input(
            BenchmarkId::new("application_chain", length),
            length,
            |b, _| {
                b.iter(|| bridge.translate_to_lean(black_box(&term)))
            }
        );
    }

    group.finish();
}

/// Benchmark concurrent translation performance
fn bench_concurrent_translation(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_translation");

    let terms: Vec<Term> = (0..100)
        .map(|i| match i % 5 {
            0 => create_church_numeral(i % 8 + 1),
            1 => create_nested_lambda(i % 10 + 3),
            2 => create_nested_application(i % 6 + 2),
            3 => create_complex_pi_type(),
            _ => Term::lambda(Term::app(Term::var(1), Term::var(0))),
        })
        .collect();

    // Sequential processing
    group.bench_function("sequential", |b| {
        let bridge = LeanBridge::new().expect("Failed to create bridge");

        b.iter(|| {
            for term in &terms {
                let _ = bridge.translate_to_lean(black_box(term));
            }
        })
    });

    // Parallel processing (using rayon)
    #[cfg(feature = "rayon")]
    group.bench_function("parallel", |b| {
        use rayon::prelude::*;
        let bridge = std::sync::Arc::new(
            LeanBridge::new().expect("Failed to create bridge")
        );

        b.iter(|| {
            terms.par_iter().for_each(|term| {
                let _ = bridge.translate_to_lean(black_box(term));
            });
        })
    });

    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    group.sample_size(20);

    // Memory usage with different term sizes
    for size in [100, 500, 1000, 2000].iter() {
        let terms: Vec<Term> = (0..*size)
            .map(|i| create_nested_lambda(i % 15 + 1))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("batch_translation", size),
            size,
            |b, _| {
                b.iter_custom(|iters| {
                    let mut total_duration = Duration::new(0, 0);

                    for _ in 0..iters {
                        // Fresh bridge to avoid cache effects on memory
                        let bridge = LeanBridge::new().expect("Failed to create bridge");

                        let start = std::time::Instant::now();
                        for term in &terms {
                            let _ = bridge.translate_to_lean(black_box(term));
                        }
                        total_duration += start.elapsed();

                        // Force garbage collection (if available)
                        drop(bridge);
                    }

                    total_duration
                })
            }
        );
    }

    group.finish();
}

/// Benchmark translation of very large terms
fn bench_large_term_translation(c: &mut Criterion) {
    let bridge = LeanBridge::new().expect("Failed to create bridge");

    let mut group = c.benchmark_group("large_terms");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    // Create increasingly large terms
    let large_terms = vec![
        ("lambda_depth_50", create_nested_lambda(50)),
        ("church_20", create_church_numeral(20)),
        ("app_chain_30", create_nested_application(30)),
        ("complex_structure", create_large_complex_term()),
    ];

    for (name, term) in large_terms {
        group.bench_function(name, |b| {
            b.iter(|| bridge.translate_to_lean(black_box(&term)))
        });
    }

    group.finish();
}

// Helper functions for creating test terms

/// Create Church numeral: λf.λx.f^n x
fn create_church_numeral(n: usize) -> Term {
    let body = (0..n).fold(
        Term::var(0), // x
        |acc, _| Term::app(Term::var(1), acc) // f (f ... (f x))
    );
    Term::lambda(Term::lambda(body))
}

/// Create nested lambda: λx₁.λx₂....λxₙ.x₁
fn create_nested_lambda(depth: usize) -> Term {
    (0..depth).fold(
        Term::var(depth - 1), // Reference outermost variable
        |acc, _| Term::lambda(acc)
    )
}

/// Create nested application: f x₁ x₂ ... xₙ
fn create_nested_application(arity: usize) -> Term {
    (0..arity).fold(
        Term::var(arity), // Function
        |acc, i| Term::app(acc, Term::var(i))
    )
}

/// Create complex dependent type
fn create_complex_pi_type() -> Term {
    // (A : Type₀) → (P : A → Type₀) → (x : A) → P x
    Term::pi(
        Term::universe(0), // A : Type₀
        Term::pi(
            Term::pi(Term::var(0), Term::universe(0)), // P : A → Type₀
            Term::pi(
                Term::var(1), // x : A
                Term::app(Term::var(1), Term::var(0)) // P x
            )
        )
    )
}

/// Create a large complex term for stress testing
fn create_large_complex_term() -> Term {
    // Create a term that combines multiple complexity factors
    let church_5 = create_church_numeral(5);
    let nested_lambda = create_nested_lambda(10);
    let complex_pi = create_complex_pi_type();

    // Combine them in a complex application structure
    Term::app(
        Term::lambda(
            Term::app(
                church_5,
                Term::app(nested_lambda, complex_pi)
            )
        ),
        Term::pi(
            Term::universe(1),
            Term::lambda(
                Term::app(
                    Term::var(1),
                    Term::lambda(Term::var(0))
                )
            )
        )
    )
}

/// Configuration for benchmarks
mod config {
    use criterion::Criterion;
    use std::time::Duration;

    pub fn configure_criterion() -> Criterion {
        Criterion::default()
            .sample_size(100)
            .measurement_time(Duration::from_secs(10))
            .warm_up_time(Duration::from_secs(3))
            .confidence_level(0.95)
            .significance_level(0.05)
    }
}