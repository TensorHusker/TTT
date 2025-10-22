//! Normalization performance benchmarks
//!
//! Benchmarks for term normalization performance, focusing on critical hot paths
//! identified during profiling: evaluation, conversion checking, and substitution.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ttt::core::Term;
use ttt::eval::normalize;

/// Create a deeply nested lambda term: λx₁.λx₂...λxₙ.x₁
fn create_nested_lambdas(depth: usize) -> Term {
    let mut term = Term::var(depth - 1); // Reference outermost variable
    for _ in 0..depth {
        term = Term::lambda(term);
    }
    term
}

/// Create a left-associative application chain: (((f x₁) x₂) x₃) ... xₙ
fn create_application_chain(length: usize) -> Term {
    let mut term = Term::var(0); // Base function
    for i in 1..=length {
        term = Term::app(term, Term::var(i));
    }
    term
}

/// Create a chain of beta reductions: (λx.e) arg
fn create_beta_chain(depth: usize) -> Term {
    let mut term = Term::var(0);
    for i in 0..depth {
        let lambda = Term::lambda(term);
        term = Term::app(lambda, Term::universe(i as u32));
    }
    term
}

/// Create a complex nested term with multiple constructors
fn create_complex_term(size: usize) -> Term {
    let base = Term::universe(0);
    let mut term = base;

    for i in 0..size {
        match i % 4 {
            0 => term = Term::lambda(term),
            1 => term = Term::app(term, Term::var(i % 3)),
            2 => term = Term::pi(term.clone(), term),
            3 => term = Term::let_in(Term::universe(1), term),
            _ => unreachable!(),
        }
    }

    term
}

/// Benchmark basic normalization operations
fn bench_basic_normalization(c: &mut Criterion) {
    let mut group = c.benchmark_group("basic_normalization");

    // Identity function normalization
    let identity = Term::lambda(Term::var(0));
    group.bench_function("identity", |b| {
        b.iter(|| normalize(black_box(&identity)))
    });

    // Simple beta reduction
    let simple_beta = Term::app(
        Term::lambda(Term::var(0)),
        Term::universe(0)
    );
    group.bench_function("simple_beta", |b| {
        b.iter(|| normalize(black_box(&simple_beta)))
    });

    // Universe normalization
    let universe = Term::universe(42);
    group.bench_function("universe", |b| {
        b.iter(|| normalize(black_box(&universe)))
    });

    group.finish();
}

/// Benchmark nested lambda normalization
fn bench_nested_lambdas(c: &mut Criterion) {
    let mut group = c.benchmark_group("nested_lambdas");
    group.throughput(Throughput::Elements(1));

    for depth in [10, 50, 100, 200].iter() {
        let term = create_nested_lambdas(*depth);
        group.bench_with_input(
            BenchmarkId::new("normalize", depth),
            depth,
            |b, _| b.iter(|| normalize(black_box(&term)))
        );
    }

    group.finish();
}

/// Benchmark application chain normalization
fn bench_application_chains(c: &mut Criterion) {
    let mut group = c.benchmark_group("application_chains");
    group.throughput(Throughput::Elements(1));

    for length in [10, 50, 100, 200].iter() {
        let term = create_application_chain(*length);
        group.bench_with_input(
            BenchmarkId::new("normalize", length),
            length,
            |b, _| b.iter(|| normalize(black_box(&term)))
        );
    }

    group.finish();
}

/// Benchmark beta reduction chains
fn bench_beta_chains(c: &mut Criterion) {
    let mut group = c.benchmark_group("beta_chains");
    group.throughput(Throughput::Elements(1));

    for depth in [5, 10, 20, 50].iter() {
        let term = create_beta_chain(*depth);
        group.bench_with_input(
            BenchmarkId::new("normalize", depth),
            depth,
            |b, _| b.iter(|| normalize(black_box(&term)))
        );
    }

    group.finish();
}

/// Benchmark complex term normalization
fn bench_complex_terms(c: &mut Criterion) {
    let mut group = c.benchmark_group("complex_terms");
    group.throughput(Throughput::Elements(1));

    for size in [20, 50, 100, 200].iter() {
        let term = create_complex_term(*size);
        group.bench_with_input(
            BenchmarkId::new("normalize", size),
            size,
            |b, _| b.iter(|| normalize(black_box(&term)))
        );
    }

    group.finish();
}

/// Benchmark substitution performance
fn bench_substitution_patterns(c: &mut Criterion) {
    use ttt::core::subst::{substitute_var, apply_substitution, Substitution};

    let mut group = c.benchmark_group("substitution");

    // Single substitution
    let term = create_nested_lambdas(50);
    let replacement = Term::universe(0);
    group.bench_function("single_var", |b| {
        b.iter(|| substitute_var(black_box(&term), 0, black_box(&replacement)))
    });

    // Multiple substitution
    let mut subst = Substitution::empty();
    for i in 0..10 {
        subst = subst.extend(i, Term::universe(i as u32));
    }
    group.bench_function("multiple_parallel", |b| {
        b.iter(|| apply_substitution(black_box(&term), black_box(&subst)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_basic_normalization,
    bench_nested_lambdas,
    bench_application_chains,
    bench_beta_chains,
    bench_complex_terms,
    bench_substitution_patterns
);

criterion_main!(benches);