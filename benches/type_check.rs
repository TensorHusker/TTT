//! Type checking performance benchmarks
//!
//! Benchmarks for type checking performance, focusing on conversion checking,
//! constraint solving, and inference hot paths.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ttt::core::{Term, Value, Environment};
use ttt::eval::{normalize, convertible, evaluate};

/// Create a chain of Pi types: A → B → C → ... → Z
fn create_pi_chain(depth: usize) -> Term {
    let mut term = Term::universe(0); // Final result type

    for i in (0..depth).rev() {
        let domain = Term::universe(i as u32);
        term = Term::pi(domain, term);
    }

    term
}

/// Create a curried function type: A → (B → (C → ... → Z))
fn create_curried_type(arity: usize) -> Term {
    let mut term = Term::universe(0);

    for i in 0..arity {
        let domain = Term::universe(i as u32);
        term = Term::pi(domain, term);
    }

    term
}

/// Create terms that should be convertible through beta reduction
fn create_convertible_pair(complexity: usize) -> (Term, Term) {
    // Create (λx.λy.x) A B which should reduce to A
    let selector = Term::lambda(Term::lambda(Term::var(1)));
    let arg1 = Term::universe(complexity as u32);
    let arg2 = Term::universe((complexity + 1) as u32);

    let lhs = Term::app(Term::app(selector, arg1.clone()), arg2);
    let rhs = arg1;

    (lhs, rhs)
}

/// Benchmark basic normalization operations
fn bench_normalization_core(c: &mut Criterion) {
    let mut group = c.benchmark_group("normalization_core");

    // Variable lookup in environment
    let env = (0..100).fold(Environment::new(), |acc, i| {
        acc.extend(Value::universe(ttt::core::Level(i)))
    });
    let var_term = Term::var(50);

    group.bench_function("env_lookup", |b| {
        b.iter(|| evaluate(black_box(&var_term), black_box(&env)))
    });

    // Deep application evaluation
    let mut app_term = Term::var(0);
    for i in 1..20 {
        app_term = Term::app(app_term, Term::var(i));
    }

    group.bench_function("deep_application", |b| {
        b.iter(|| evaluate(black_box(&app_term), black_box(&env)))
    });

    group.finish();
}

/// Benchmark conversion checking performance
fn bench_convertibility(c: &mut Criterion) {
    let mut group = c.benchmark_group("convertibility");
    group.throughput(Throughput::Elements(1));

    for complexity in [5, 10, 20, 50].iter() {
        let (lhs, rhs) = create_convertible_pair(*complexity);

        group.bench_with_input(
            BenchmarkId::new("beta_equal", complexity),
            complexity,
            |b, _| b.iter(|| convertible(black_box(&lhs), black_box(&rhs)))
        );
    }

    // Test identical terms (should be fast)
    let identical = Term::lambda(Term::app(Term::var(1), Term::var(0)));
    group.bench_function("identical", |b| {
        b.iter(|| convertible(black_box(&identical), black_box(&identical)))
    });

    // Test trivially different terms
    let term1 = Term::universe(0);
    let term2 = Term::universe(1);
    group.bench_function("trivially_different", |b| {
        b.iter(|| convertible(black_box(&term1), black_box(&term2)))
    });

    group.finish();
}

/// Benchmark Pi type operations
fn bench_pi_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("pi_types");
    group.throughput(Throughput::Elements(1));

    for depth in [5, 10, 20, 50].iter() {
        let pi_chain = create_pi_chain(*depth);

        group.bench_with_input(
            BenchmarkId::new("normalize_chain", depth),
            depth,
            |b, _| b.iter(|| normalize(black_box(&pi_chain)))
        );
    }

    for arity in [5, 10, 20, 50].iter() {
        let curried = create_curried_type(*arity);

        group.bench_with_input(
            BenchmarkId::new("normalize_curried", arity),
            arity,
            |b, _| b.iter(|| normalize(black_box(&curried)))
        );
    }

    group.finish();
}

/// Benchmark environment operations
fn bench_environment_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("environment_ops");

    // Environment extension
    let base_env = Environment::new();
    group.bench_function("extend_empty", |b| {
        b.iter(|| {
            black_box(&base_env).extend(black_box(Value::universe(ttt::core::Level(0))))
        })
    });

    // Large environment extension
    let large_env = (0..100).fold(Environment::new(), |acc, i| {
        acc.extend(Value::universe(ttt::core::Level(i)))
    });
    group.bench_function("extend_large", |b| {
        b.iter(|| {
            black_box(&large_env).extend(black_box(Value::universe(ttt::core::Level(999))))
        })
    });

    // Environment lookup patterns
    group.bench_function("lookup_recent", |b| {
        b.iter(|| black_box(&large_env).lookup(0)) // Most recent
    });

    group.bench_function("lookup_distant", |b| {
        b.iter(|| black_box(&large_env).lookup(99)) // Oldest
    });

    group.finish();
}

/// Benchmark structural operations
fn bench_structural_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("structural_ops");

    // Term cloning (Rc overhead)
    let complex_term = create_curried_type(20);
    group.bench_function("clone_complex", |b| {
        b.iter(|| black_box(&complex_term).clone())
    });

    // Hash computation
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    group.bench_function("hash_complex", |b| {
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            black_box(&complex_term).hash(&mut hasher);
            hasher.finish()
        })
    });

    // Equality checking
    let term_copy = complex_term.clone();
    group.bench_function("equality_identical", |b| {
        b.iter(|| black_box(&complex_term) == black_box(&term_copy))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_normalization_core,
    bench_convertibility,
    bench_pi_types,
    bench_environment_ops,
    bench_structural_ops
);

criterion_main!(benches);