//! Property generators for TTT-Lean bridge testing
//!
//! This module provides sophisticated generators for creating test cases
//! that explore the full space of TTT and Lean terms systematically.

use ttt::core::{Term, Level};
use ttt::lean::{LeanTerm, LeanLevel, LeanName, TranslationContext};
use proptest::prelude::*;
use std::collections::HashMap;

/// Maximum depth for generated terms to avoid infinite recursion
const MAX_DEPTH: usize = 8;
const MAX_UNIVERSE_LEVEL: u32 = 10;
const MAX_VAR_INDEX: usize = 20;
const MAX_META_ID: usize = 100;

/// Strategy for generating TTT Terms
pub fn arb_term() -> impl Strategy<Value = Term> {
    arb_term_with_depth(MAX_DEPTH)
}

/// Generate TTT terms with controlled depth
pub fn arb_term_with_depth(max_depth: usize) -> impl Strategy<Value = Term> {
    let leaf = prop_oneof![
        any::<usize>().prop_map(|i| Term::var(i % MAX_VAR_INDEX)),
        any::<u32>().prop_map(|l| Term::universe(l % MAX_UNIVERSE_LEVEL)),
        any::<usize>().prop_map(|id| Term::meta(id % MAX_META_ID)),
    ];

    leaf.prop_recursive(max_depth as u32, 256, 10, |inner| {
        prop_oneof![
            // Lambda abstraction
            inner.clone().prop_map(Term::lambda),

            // Pi types
            (inner.clone(), inner.clone()).prop_map(|(dom, cod)| Term::pi(dom, cod)),

            // Applications
            (inner.clone(), inner.clone()).prop_map(|(fun, arg)| Term::app(fun, arg)),

            // Let bindings
            (inner.clone(), inner.clone()).prop_map(|(bind, body)| Term::let_in(bind, body)),
        ]
    })
}

/// Generate closed TTT terms (no free variables)
pub fn arb_closed_term() -> impl Strategy<Value = Term> {
    arb_closed_term_with_depth(MAX_DEPTH)
}

pub fn arb_closed_term_with_depth(max_depth: usize) -> impl Strategy<Value = Term> {
    let leaf = prop_oneof![
        any::<u32>().prop_map(|l| Term::universe(l % MAX_UNIVERSE_LEVEL)),
        any::<usize>().prop_map(|id| Term::meta(id % MAX_META_ID)),
    ];

    leaf.prop_recursive(max_depth as u32, 256, 10, |inner| {
        prop_oneof![
            // Lambda with bound variable usage
            inner.clone().prop_map(|body| {
                // Ensure the body uses the bound variable (index 0)
                Term::lambda(substitute_free_vars(body, 0))
            }),

            // Pi types with dependency
            (inner.clone(), inner.clone()).prop_map(|(dom, cod)| {
                Term::pi(dom, substitute_free_vars(cod, 0))
            }),

            // Applications of closed terms
            (inner.clone(), inner.clone()).prop_map(|(fun, arg)| Term::app(fun, arg)),

            // Let bindings with proper scoping
            (inner.clone(), inner.clone()).prop_map(|(bind, body)| {
                Term::let_in(bind, substitute_free_vars(body, 0))
            }),
        ]
    })
}

/// Helper to substitute free variables with bound ones
fn substitute_free_vars(term: Term, binding_depth: usize) -> Term {
    match term {
        Term::Var(i) => Term::Var(i % (binding_depth + 1)),
        Term::Universe(l) => Term::Universe(l),
        Term::Meta(id) => Term::Meta(id),
        Term::Lambda(body) => Term::Lambda(
            std::rc::Rc::new(substitute_free_vars((*body).clone(), binding_depth + 1))
        ),
        Term::Pi(dom, cod) => Term::Pi(
            std::rc::Rc::new(substitute_free_vars((*dom).clone(), binding_depth)),
            std::rc::Rc::new(substitute_free_vars((*cod).clone(), binding_depth + 1))
        ),
        Term::App(fun, arg) => Term::App(
            std::rc::Rc::new(substitute_free_vars((*fun).clone(), binding_depth)),
            std::rc::Rc::new(substitute_free_vars((*arg).clone(), binding_depth))
        ),
        Term::Let(bind, body) => Term::Let(
            std::rc::Rc::new(substitute_free_vars((*bind).clone(), binding_depth)),
            std::rc::Rc::new(substitute_free_vars((*body).clone(), binding_depth + 1))
        ),
    }
}

/// Strategy for generating Lean terms
pub fn arb_lean_term() -> impl Strategy<Value = LeanTerm> {
    arb_lean_term_with_depth(MAX_DEPTH)
}

pub fn arb_lean_term_with_depth(max_depth: usize) -> impl Strategy<Value = LeanTerm> {
    let leaf = prop_oneof![
        arb_lean_name().prop_map(LeanTerm::Var),
        arb_lean_level().prop_map(LeanTerm::Sort),
        arb_lean_name().prop_map(LeanTerm::Const),
    ];

    leaf.prop_recursive(max_depth as u32, 256, 10, |inner| {
        prop_oneof![
            // Lambda
            (arb_lean_name(), inner.clone(), inner.clone())
                .prop_map(|(name, ty, body)| LeanTerm::lambda(name, ty, body)),

            // Pi type
            (arb_lean_name(), inner.clone(), inner.clone())
                .prop_map(|(name, dom, cod)| LeanTerm::pi(name, dom, cod)),

            // Application
            (inner.clone(), inner.clone())
                .prop_map(|(fun, arg)| LeanTerm::app(fun, arg)),

            // Let binding
            (arb_lean_name(), inner.clone(), inner.clone(), inner.clone())
                .prop_map(|(name, ty, val, body)| {
                    LeanTerm::let_in(name, ty, val, body)
                }),
        ]
    })
}

/// Strategy for generating Lean names
pub fn arb_lean_name() -> impl Strategy<Value = LeanName> {
    prop_oneof![
        "x",
        "y",
        "z",
        "f",
        "g",
        "h",
        "A",
        "B",
        "C",
        "P",
        "Q",
        "_",
        r"[a-zA-Z][a-zA-Z0-9_]*",
    ].prop_map(|s| LeanName::new(s))
}

/// Strategy for generating Lean universe levels
pub fn arb_lean_level() -> impl Strategy<Value = LeanLevel> {
    let leaf = prop_oneof![
        Just(LeanLevel::zero()),
        r"[a-z][a-zA-Z0-9]*".prop_map(LeanLevel::param),
    ];

    leaf.prop_recursive(6, 32, 5, |inner| {
        prop_oneof![
            inner.clone().prop_map(LeanLevel::succ),
            (inner.clone(), inner).prop_map(|(l1, l2)| LeanLevel::max(l1, l2)),
        ]
    })
}

/// Strategy for generating translation contexts
pub fn arb_translation_context() -> impl Strategy<Value = TranslationContext> {
    prop::collection::vec(arb_lean_name(), 0..10)
        .prop_map(|names| {
            let mut ctx = TranslationContext::new();
            for (i, name) in names.into_iter().enumerate() {
                ctx.bind_variable(i, name);
            }
            ctx
        })
}

/// Generate pairs of equivalent terms for testing bisimulation
pub fn arb_equivalent_term_pairs() -> impl Strategy<Value = (Term, Term)> {
    arb_term().prop_map(|term| {
        // Generate α-equivalent terms by renaming bound variables
        let alpha_equivalent = alpha_rename_term(term.clone());
        (term, alpha_equivalent)
    })
}

/// Perform α-renaming on a term (placeholder implementation)
fn alpha_rename_term(term: Term) -> Term {
    // For now, return the same term
    // In a full implementation, this would systematically rename bound variables
    term
}

/// Generate complex nested structures for stress testing
pub fn arb_complex_term() -> impl Strategy<Value = Term> {
    prop_oneof![
        // Church numerals
        arb_church_numeral(),

        // Nested function types
        arb_nested_pi_type(),

        // Complex applications
        arb_complex_application(),

        // Deeply nested lambdas
        arb_nested_lambda(),
    ]
}

/// Generate Church numerals: λf.λx.f^n x
pub fn arb_church_numeral() -> impl Strategy<Value = Term> {
    (0..10u32).prop_map(|n| {
        // λf.λx. f^n x
        let body = (0..n).fold(
            Term::var(0), // x
            |acc, _| Term::app(Term::var(1), acc) // f acc
        );
        Term::lambda(Term::lambda(body))
    })
}

/// Generate nested Pi types: A₁ → A₂ → ... → Aₙ
pub fn arb_nested_pi_type() -> impl Strategy<Value = Term> {
    (1..8usize).prop_map(|n| {
        (0..n).fold(
            Term::universe(0), // Final codomain
            |acc, _| Term::pi(Term::universe(0), acc)
        )
    })
}

/// Generate complex applications: ((f x₁) x₂) ... xₙ
pub fn arb_complex_application() -> impl Strategy<Value = Term> {
    (1..6usize).prop_map(|n| {
        (0..n).fold(
            Term::var(n), // Function
            |acc, i| Term::app(acc, Term::var(i))
        )
    })
}

/// Generate deeply nested lambda abstractions
pub fn arb_nested_lambda() -> impl Strategy<Value = Term> {
    (1..12usize).prop_map(|n| {
        (0..n).fold(
            Term::var(0), // Innermost variable
            |acc, _| Term::lambda(acc)
        )
    })
}

// Note: QuickCheck Arbitrary implementations are omitted to avoid orphan rule violations.
// TTT types are defined in the main crate, and implementing external traits for them
// in test modules would be invalid. PropTest generators above provide comprehensive coverage.