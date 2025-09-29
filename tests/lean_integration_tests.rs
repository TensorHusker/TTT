//! Comprehensive integration tests for TTT-Lean bridge
//!
//! This module tests the bidirectional translation between TTT terms and Lean terms,
//! ensuring mathematical correctness and structural preservation.

#![cfg(feature = "lean-integration")]

use ttt::core::{Term, Level};
use ttt::lean::{LeanTerm, LeanLevel, LeanName, LeanTranslator};
use pretty_assertions::assert_eq;
use std::collections::HashSet;

use crate::generators::*;

/// Create a test translator for testing
fn create_test_translator() -> LeanTranslator {
    LeanTranslator::new()
}

/// Test basic universe translation roundtrips
#[test]
fn test_universe_translation() {
    let translator = create_test_translator();

    // Test Type₀
    let type_0 = Term::type_0();
    let lean_type_0 = translator.to_lean(&type_0).unwrap();
    let back_to_ttt = translator.from_lean(&lean_type_0).unwrap();
    assert_eq!(type_0, back_to_ttt);

    // Test Type₁
    let type_1 = Term::type_1();
    let lean_type_1 = translator.to_lean(&type_1).unwrap();
    let back_to_ttt = translator.from_lean(&lean_type_1).unwrap();
    assert_eq!(type_1, back_to_ttt);

    // Test higher universes
    for level in 0..10 {
        let universe = Term::universe(level);
        let lean_universe = translator.to_lean(&universe).unwrap();

        match lean_universe {
            LeanTerm::Sort(lean_level) => {
                assert_eq!(lean_level.to_nat(), Some(level));
            },
            _ => panic!("Expected Sort, got {:?}", lean_universe),
        }

        let back_to_ttt = translator.from_lean(&lean_universe).unwrap();
        assert_eq!(universe, back_to_ttt);
    }
}

/// Test variable translation with De Bruijn indices
#[test]
fn test_variable_translation() {
    let translator = create_test_translator();

    // Test simple variable
    let var_0 = Term::var(0);
    let lean_var = translator.to_lean(&var_0).unwrap();

    match lean_var {
        LeanTerm::Var(name) => {
            assert!(name.as_str().starts_with("x_"));
        },
        _ => panic!("Expected Var, got {:?}", lean_var),
    }

    let back_to_ttt = translator.from_lean(&lean_var).unwrap();
    assert_eq!(var_0, back_to_ttt);

    // Test multiple variables with different indices
    for index in 0..5 {
        let var = Term::var(index);
        let lean_var = translator.to_lean(&var).unwrap();
        let back_to_ttt = translator.from_lean(&lean_var).unwrap();
        assert_eq!(var, back_to_ttt);
    }
}

/// Test lambda term translation
#[test]
fn test_lambda_translation() {
    let translator = create_test_translator();

    // Identity function: λx.x
    let identity = Term::lambda(Term::var(0));
    let lean_identity = translator.to_lean(&identity).unwrap();

    match lean_identity {
        LeanTerm::Lambda(name, ty, body) => {
            // Check that bound variable is used correctly
            match &**body {
                LeanTerm::Var(body_name) => {
                    assert_eq!(name, *body_name);
                },
                _ => panic!("Expected variable in lambda body"),
            }
        },
        _ => panic!("Expected Lambda, got {:?}", lean_identity),
    }

    let back_to_ttt = translator.from_lean(&lean_identity).unwrap();
    assert_eq!(identity, back_to_ttt);
}

/// Test Pi type translation
#[test]
fn test_pi_translation() {
    let translator = create_test_translator();

    // Simple function type: A → B (non-dependent)
    let simple_pi = Term::pi(Term::var(1), Term::var(0));
    let lean_pi = translator.to_lean(&simple_pi).unwrap();

    match lean_pi {
        LeanTerm::Pi(name, domain, codomain) => {
            // Verify structure is preserved
            assert!(matches!(**domain, LeanTerm::Var(_)));
            assert!(matches!(**codomain, LeanTerm::Var(_)));
        },
        _ => panic!("Expected Pi, got {:?}", lean_pi),
    }

    let back_to_ttt = translator.from_lean(&lean_pi).unwrap();
    assert_eq!(simple_pi, back_to_ttt);

    // Dependent function type
    let dependent_pi = Term::pi(Term::type_0(), Term::pi(Term::var(0), Term::var(1)));
    let lean_dependent = translator.to_lean(&dependent_pi).unwrap();
    let back_to_ttt = translator.from_lean(&lean_dependent).unwrap();
    assert_eq!(dependent_pi, back_to_ttt);
}

/// Test function application translation
#[test]
fn test_application_translation() {
    let translator = create_test_translator();

    // Simple application: f x
    let app = Term::app(Term::var(1), Term::var(0));
    let lean_app = translator.to_lean(&app).unwrap();

    match lean_app {
        LeanTerm::App(func, arg) => {
            assert!(matches!(**func, LeanTerm::Var(_)));
            assert!(matches!(**arg, LeanTerm::Var(_)));
        },
        _ => panic!("Expected App, got {:?}", lean_app),
    }

    let back_to_ttt = translator.from_lean(&lean_app).unwrap();
    assert_eq!(app, back_to_ttt);

    // Nested application: ((f x) y)
    let nested_app = Term::app(
        Term::app(Term::var(2), Term::var(1)),
        Term::var(0)
    );
    let lean_nested = translator.to_lean(&nested_app).unwrap();
    let back_to_ttt = translator.from_lean(&lean_nested).unwrap();
    assert_eq!(nested_app, back_to_ttt);
}

/// Test let binding translation
#[test]
fn test_let_translation() {
    let translator = create_test_translator();

    // let x = y in x
    let let_term = Term::let_in(Term::var(0), Term::var(0));
    let lean_let = translator.to_lean(&let_term).unwrap();

    match lean_let {
        LeanTerm::Let(name, ty, val, body) => {
            // Check structure preservation
            assert!(matches!(**val, LeanTerm::Var(_)));
            assert!(matches!(**body, LeanTerm::Var(_)));

            // Check that bound variable is referenced correctly
            match &**body {
                LeanTerm::Var(body_name) => {
                    assert_eq!(name, *body_name);
                },
                _ => panic!("Expected variable in let body"),
            }
        },
        _ => panic!("Expected Let, got {:?}", lean_let),
    }

    let back_to_ttt = translator.from_lean(&lean_let).unwrap();
    assert_eq!(let_term, back_to_ttt);
}

/// Test complex nested term translation
#[test]
fn test_complex_nested_translation() {
    let translator = create_test_translator();

    // Church numeral 2: λf.λx.f (f x)
    let church_2 = Term::lambda(
        Term::lambda(
            Term::app(
                Term::var(1),  // f
                Term::app(
                    Term::var(1),  // f
                    Term::var(0)   // x
                )
            )
        )
    );

    let lean_church_2 = translator.to_lean(&church_2).unwrap();
    let back_to_ttt = translator.from_lean(&lean_church_2).unwrap();
    assert_eq!(church_2, back_to_ttt);

    // Dependent pair type: Σ(x:A).B(x)
    let sigma_type = Term::pi(
        Term::type_0(),  // A : Type₀
        Term::pi(
            Term::var(0),  // x : A
            Term::type_0() // B(x) : Type₀
        )
    );

    let lean_sigma = translator.to_lean(&sigma_type).unwrap();
    let back_to_ttt = translator.from_lean(&lean_sigma).unwrap();
    assert_eq!(sigma_type, back_to_ttt);
}

/// Test translation context management
#[test]
fn test_context_management() {
    let translator = create_test_translator();

    // Test with deeply nested binding context
    let deeply_nested = (0..10).fold(Term::var(9), |body, _| {
        Term::lambda(body)
    });

    let lean_nested = translator.to_lean(&deeply_nested).unwrap();
    let back_to_ttt = translator.from_lean(&lean_nested).unwrap();
    assert_eq!(deeply_nested, back_to_ttt);
}

/// Test translation of terms with metavariables
#[test]
fn test_metavariable_translation() {
    let translator = create_test_translator();

    // Simple metavariable
    let meta = Term::meta(42);
    let lean_meta = translator.to_lean(&meta).unwrap();

    // Metavariables should be translated to special constants
    match lean_meta {
        LeanTerm::Const(name) => {
            assert!(name.as_str().starts_with("?"));
        },
        _ => panic!("Expected Const for metavariable, got {:?}", lean_meta),
    }

    let back_to_ttt = translator.from_lean(&lean_meta).unwrap();
    assert_eq!(meta, back_to_ttt);

    // Term with embedded metavariables
    let term_with_meta = Term::app(Term::meta(1), Term::meta(2));
    let lean_with_meta = translator.to_lean(&term_with_meta).unwrap();
    let back_to_ttt = translator.from_lean(&lean_with_meta).unwrap();
    assert_eq!(term_with_meta, back_to_ttt);
}

/// Test error handling for malformed inputs
#[test]
fn test_error_handling() {
    let translator = create_test_translator();

    // Test with invalid Lean terms (would need to construct invalid terms)
    // This would require extending the Lean types to support invalid states

    // For now, test that all valid terms translate successfully
    let valid_terms = vec![
        Term::var(0),
        Term::type_0(),
        Term::type_1(),
        Term::universe(42),
        Term::lambda(Term::var(0)),
        Term::pi(Term::type_0(), Term::var(0)),
        Term::app(Term::var(1), Term::var(0)),
        Term::let_in(Term::var(1), Term::var(0)),
        Term::meta(123),
    ];

    for term in valid_terms {
        let lean_term = translator.to_lean(&term).unwrap();
        let back_to_ttt = translator.from_lean(&lean_term).unwrap();
        assert_eq!(term, back_to_ttt);
    }
}

/// Test cache functionality
#[test]
fn test_translation_cache() {
    let translator = create_test_translator();

    let term = Term::lambda(Term::app(Term::var(1), Term::var(0)));

    // First translation - cache miss
    let lean_term1 = translator.to_lean(&term).unwrap();
    let initial_cache_rate = bridge.metrics().cache_hit_rate();

    // Second translation - should hit cache
    let lean_term2 = translator.to_lean(&term).unwrap();
    let final_cache_rate = bridge.metrics().cache_hit_rate();

    assert_eq!(lean_term1, lean_term2);
    assert!(final_cache_rate >= initial_cache_rate);
    assert!(bridge.metrics().translation_count() >= 2);
}

/// Test performance with various term sizes
#[test]
fn test_performance_scaling() {
    let translator = create_test_translator();

    // Test with increasing term complexity
    let term_sizes = vec![10, 50, 100];

    for size in term_sizes {
        let large_term = create_nested_lambda_term(size);

        let start = std::time::Instant::now();
        let lean_term = translator.to_lean(&large_term).unwrap();
        let translation_time = start.elapsed();

        let start = std::time::Instant::now();
        let back_to_ttt = translator.from_lean(&lean_term).unwrap();
        let back_translation_time = start.elapsed();

        assert_eq!(large_term, back_to_ttt);

        println!("Size {}: Translation {}μs, Back-translation {}μs",
                 size,
                 translation_time.as_micros(),
                 back_translation_time.as_micros());
    }
}

/// Helper function to create nested lambda terms of specified depth
fn create_nested_lambda_term(depth: usize) -> Term {
    (0..depth).fold(Term::var(0), |body, _| Term::lambda(body))
}

/// Test structural equivalence preservation
#[test]
fn test_structural_preservation() {
    let translator = create_test_translator();

    let terms = vec![
        // α-equivalent terms should have same structure after translation
        Term::lambda(Term::var(0)),
        Term::lambda(Term::var(0)), // Same as above

        // Different structures should remain different
        Term::lambda(Term::lambda(Term::var(1))),
        Term::lambda(Term::lambda(Term::var(0))),
    ];

    for term in terms {
        let lean_term = translator.to_lean(&term).unwrap();

        // Verify structural properties are preserved
        match (&term, &lean_term) {
            (Term::Lambda(_), LeanTerm::Lambda(_, _, _)) => (),
            (Term::Pi(_, _), LeanTerm::Pi(_, _, _)) => (),
            (Term::App(_, _), LeanTerm::App(_, _)) => (),
            (Term::Var(_), LeanTerm::Var(_)) => (),
            (Term::Universe(_), LeanTerm::Sort(_)) => (),
            (Term::Let(_, _), LeanTerm::Let(_, _, _, _)) => (),
            (Term::Meta(_), LeanTerm::Const(_)) => (),
            _ => panic!("Structure not preserved: {:?} -> {:?}", term, lean_term),
        }

        let back_to_ttt = translator.from_lean(&lean_term).unwrap();
        assert_eq!(term, back_to_ttt);
    }
}

/// Integration test for end-to-end translation pipeline
#[test]
fn test_end_to_end_pipeline() {
    let translator = create_test_translator();

    // Create a complex term representing a dependent function
    let complex_term = Term::pi(
        Term::universe(0),  // A : Type₀
        Term::pi(
            Term::pi(Term::var(0), Term::universe(0)),  // P : A → Type₀
            Term::pi(
                Term::pi(
                    Term::var(1),  // x : A
                    Term::app(Term::var(1), Term::var(0))  // P x
                ),
                Term::pi(
                    Term::var(2),  // y : A
                    Term::app(Term::var(2), Term::var(0))  // P y
                )
            )
        )
    );

    // Test full pipeline: TTT → Lean → TTT
    let lean_complex = translator.to_lean(&complex_term).unwrap();
    let recovered_term = translator.from_lean(&lean_complex).unwrap();

    assert_eq!(complex_term, recovered_term);

    // Verify metrics were recorded
    assert!(bridge.metrics().translation_count() > 0);
}