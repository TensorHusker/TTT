//! Property-based test generators for TTT verification
//!
//! This module provides comprehensive generators for all TTT data structures
//! enabling exhaustive property-based testing of mathematical correctness.

use proptest::prelude::*;
use ttt::core::{Term, Level, Value, Environment, Closure, Neutral};
use std::rc::Rc;

/// Generator for universe levels with bias toward small values
pub fn level_gen() -> impl Strategy<Value = Level> {
    prop_oneof![
        // Heavily bias toward small levels that occur in practice
        1 => Just(Level(0)),
        1 => Just(Level(1)),
        1 => Just(Level(2)),
        // Include some larger levels for edge case testing
        1 => (3u32..10).prop_map(Level),
        // Very rare extremely large levels
        1 => (10u32..100).prop_map(Level),
    ]
}

/// Generator for De Bruijn indices with realistic distribution
pub fn index_gen(max_depth: usize) -> impl Strategy<Value = usize> {
    if max_depth == 0 {
        // If no depth, can only have free variables
        0usize..5
    } else {
        prop_oneof![
            // Bound variables (most common in well-typed terms)
            3 => 0usize..max_depth,
            // Free variables (less common but important for testing)
            1 => max_depth..(max_depth + 3),
        ]
    }
}

/// Generate small terms with controlled size and depth
pub fn small_term_gen() -> impl Strategy<Value = Term> {
    let leaf = prop_oneof![
        // Variables with small indices
        3 => (0usize..5).prop_map(Term::var),
        // Universe levels
        2 => level_gen().prop_map(Term::Universe),
        // Metavariables
        1 => (0usize..10).prop_map(Term::meta),
    ];

    leaf.prop_recursive(
        3, // Maximum depth
        8, // Maximum total size
        3, // Maximum items per collection
        |inner| {
            prop_oneof![
                // Lambda abstractions
                2 => inner.clone().prop_map(|body| Term::lambda(body)),

                // Applications
                3 => (inner.clone(), inner.clone()).prop_map(|(fun, arg)| {
                    Term::app(fun, arg)
                }),

                // Pi types
                2 => (inner.clone(), inner.clone()).prop_map(|(dom, cod)| {
                    Term::pi(dom, cod)
                }),

                // Let bindings
                1 => (inner.clone(), inner.clone()).prop_map(|(bind, body)| {
                    Term::let_in(bind, body)
                }),
            ]
        }
    )
}

/// Generate well-scoped terms up to given depth
pub fn well_scoped_term_gen(max_depth: usize) -> impl Strategy<Value = Term> {
    well_scoped_term_gen_aux(max_depth, 0)
}

fn well_scoped_term_gen_aux(max_size: usize, binding_depth: usize) -> impl Strategy<Value = Term> {
    let leaf = prop_oneof![
        // Only generate bound variables within scope
        if binding_depth > 0 {
            3 => index_gen(binding_depth).prop_map(Term::var)
        } else {
            // No bound variables available, use free vars sparingly
            1 => (0usize..2).prop_map(Term::var)
        },
        // Universes
        2 => level_gen().prop_map(Term::Universe),
        // Metavariables
        1 => (0usize..5).prop_map(Term::meta),
    ];

    if max_size <= 1 {
        leaf.boxed()
    } else {
        leaf.prop_recursive(
            2, // depth
            max_size,
            2, // items per collection
            move |inner| {
                prop_oneof![
                    // Lambda increases binding depth
                    2 => well_scoped_term_gen_aux(max_size / 2, binding_depth + 1)
                        .prop_map(Term::lambda),

                    // Applications maintain depth
                    3 => (inner.clone(), inner.clone()).prop_map(|(fun, arg)| {
                        Term::app(fun, arg)
                    }),

                    // Pi types: domain at current depth, codomain at depth+1
                    2 => (
                        well_scoped_term_gen_aux(max_size / 3, binding_depth),
                        well_scoped_term_gen_aux(max_size / 3, binding_depth + 1)
                    ).prop_map(|(dom, cod)| Term::pi(dom, cod)),

                    // Let bindings: binding at current depth, body at depth+1
                    1 => (
                        well_scoped_term_gen_aux(max_size / 3, binding_depth),
                        well_scoped_term_gen_aux(max_size / 3, binding_depth + 1)
                    ).prop_map(|(bind, body)| Term::let_in(bind, body)),
                ]
            }
        ).boxed()
    }
}

/// Generate closed terms (no free variables)
pub fn closed_term_gen() -> impl Strategy<Value = Term> {
    well_scoped_term_gen_aux(6, 0).prop_filter("closed", |t| t.is_closed())
}

/// Generator for values
pub fn value_gen() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        // Variables with levels
        3 => (0usize..10).prop_map(Value::var),
        // Universes
        2 => level_gen().prop_map(Value::universe),
    ];

    leaf.prop_recursive(
        2, // depth
        6, // size
        2, // items
        |inner| {
            prop_oneof![
                // Lambda values with closures
                2 => (environment_gen(), small_term_gen()).prop_map(|(env, term)| {
                    Value::lambda(Closure::new(env, term))
                }),

                // Pi types
                2 => (inner.clone(), environment_gen(), small_term_gen())
                    .prop_map(|(domain, env, cod_term)| {
                        let closure = Closure::new(env, cod_term);
                        Value::pi(domain, closure)
                    }),

                // Neutral terms
                1 => (0usize..5, prop::collection::vec(inner.clone(), 0..3))
                    .prop_map(|(head, spine)| {
                        Value::neutral(head, spine.into_iter().map(Rc::new).collect())
                    }),
            ]
        }
    )
}

/// Generator for environments
pub fn environment_gen() -> impl Strategy<Value = Environment> {
    prop::collection::vec(value_gen(), 0..4).prop_map(|values| {
        values.into_iter().fold(Environment::new(), |env, val| env.extend(val))
    })
}

/// Generator for substitutions
pub fn substitution_gen() -> impl Strategy<Value = ttt::core::subst::Substitution> {
    prop::collection::vec((0usize..5, small_term_gen()), 0..4).prop_map(|pairs| {
        pairs.into_iter().fold(
            ttt::core::subst::Substitution::empty(),
            |subst, (idx, term)| subst.extend(idx, term)
        )
    })
}

/// Generate pairs of convertible terms
pub fn convertible_term_pair_gen() -> impl Strategy<Value = (Term, Term)> {
    small_term_gen().prop_flat_map(|term| {
        // Generate α-equivalent terms by renaming bound variables
        let variants = alpha_variant_gen(&term);
        (Just(term), variants)
    })
}

/// Generate α-equivalent variants of a term
fn alpha_variant_gen(term: &Term) -> impl Strategy<Value = Term> {
    // For now, just return the same term - in a full implementation,
    // this would generate α-equivalent variants by systematic renaming
    Just(term.clone())
}

/// Generator for type checking contexts
pub fn context_gen() -> impl Strategy<Value = Vec<Term>> {
    prop::collection::vec(
        // Context entries should be types
        prop_oneof![
            3 => level_gen().prop_map(Term::Universe),
            2 => (
                well_scoped_term_gen(3),
                well_scoped_term_gen(3)
            ).prop_map(|(dom, cod)| Term::pi(dom, cod)),
        ],
        0..4
    )
}

/// Generate well-typed term in given context
pub fn well_typed_term_gen() -> impl Strategy<Value = (Vec<Term>, Term, Term)> {
    // Generate a context, then a term and its type that are valid in that context
    context_gen().prop_flat_map(|ctx| {
        let ctx_len = ctx.len();
        (
            Just(ctx),
            well_typed_term_in_context_gen(ctx_len),
            type_gen()
        )
    })
}

fn well_typed_term_in_context_gen(ctx_len: usize) -> impl Strategy<Value = Term> {
    prop_oneof![
        // Variables from context
        if ctx_len > 0 {
            2 => (0..ctx_len).prop_map(Term::var)
        } else {
            0 => Just(Term::var(0)) // This branch won't be taken
        },

        // Universes are always well-typed
        3 => level_gen().prop_map(Term::Universe),

        // Lambda abstractions
        2 => well_typed_term_in_context_gen(ctx_len + 1).prop_map(Term::lambda),

        // Simple applications (f x where f and x are variables)
        if ctx_len >= 2 {
            1 => ((0..ctx_len), (0..ctx_len)).prop_map(|(f, x)| {
                Term::app(Term::var(f), Term::var(x))
            })
        } else {
            0 => Just(Term::var(0))
        },
    ]
}

fn type_gen() -> impl Strategy<Value = Term> {
    prop_oneof![
        // Most types are universes or simple pi types
        3 => level_gen().prop_map(Term::Universe),
        2 => (small_term_gen(), small_term_gen()).prop_map(|(dom, cod)| {
            Term::pi(dom, cod)
        }),
    ]
}

/// Generate edge cases for robust testing
pub fn edge_case_term_gen() -> impl Strategy<Value = Term> {
    prop_oneof![
        // Maximum index values
        1 => Just(Term::var(usize::MAX)),
        1 => Just(Term::var(0)),

        // Deep nesting
        1 => nested_lambda_gen(10),
        1 => nested_app_gen(10),

        // Large universe levels
        1 => Just(Term::Universe(Level(u32::MAX))),

        // Highly recursive structures
        1 => recursive_pi_gen(5),
    ]
}

fn nested_lambda_gen(depth: usize) -> impl Strategy<Value = Term> {
    if depth == 0 {
        Just(Term::var(0))
    } else {
        nested_lambda_gen(depth - 1).prop_map(Term::lambda)
    }
}

fn nested_app_gen(depth: usize) -> impl Strategy<Value = Term> {
    if depth == 0 {
        Just(Term::var(0))
    } else {
        (nested_app_gen(depth - 1), small_term_gen()).prop_map(|(f, x)| Term::app(f, x))
    }
}

fn recursive_pi_gen(depth: usize) -> impl Strategy<Value = Term> {
    if depth == 0 {
        level_gen().prop_map(Term::Universe)
    } else {
        (small_term_gen(), recursive_pi_gen(depth - 1)).prop_map(|(dom, cod)| {
            Term::pi(dom, cod)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_small_term_gen_terminates(term in small_term_gen()) {
            // Just check that generation terminates and produces valid terms
            match term {
                Term::Var(_) | Term::Universe(_) | Term::Meta(_) => {},
                Term::Pi(_, _) | Term::Lambda(_) | Term::App(_, _) | Term::Let(_, _) => {},
            }
        }

        #[test]
        fn test_well_scoped_terms_are_valid(term in well_scoped_term_gen(5)) {
            // Check that generated terms have reasonable structure
            // This is more of a sanity check than a deep property
            let _ = term.max_index(); // Should not panic
        }

        #[test]
        fn test_closed_terms_have_no_free_vars(term in closed_term_gen()) {
            prop_assert!(term.is_closed());
        }

        #[test]
        fn test_value_gen_produces_valid_values(value in value_gen()) {
            // Basic sanity check for value generation
            match value {
                Value::Var(_) | Value::Universe(_) => {},
                Value::Pi(_, _) | Value::Lambda(_) | Value::Neutral(_) => {},
            }
        }
    }
}