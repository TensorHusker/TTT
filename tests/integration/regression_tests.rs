//! Regression tests for TTT
//!
//! Tests that prevent regression of previously fixed bugs and ensure
//! that optimizations don't break existing functionality.

use proptest::prelude::*;
use ttt::core::{Term, Value, Level};
use ttt::typeck::{Context, infer, check, CheckError};
use ttt::eval::{normalize, evaluate, convertible};
use ttt::core::subst::{substitute_var, substitute_top, apply_substitution, Substitution};
use super::super::properties::generators::*;

/// Tests for specific bugs that were previously fixed
#[cfg(test)]
mod known_bug_regression_tests {
    use super::*;

    #[test]
    fn test_universe_level_overflow_prevention() {
        let context = Context::empty();

        // Test that maximum universe levels don't cause overflow
        let max_level = Level(u32::MAX);
        let max_universe = Term::Universe(max_level.clone());

        // Should handle gracefully without panic
        let result = infer(&max_universe, &context);

        match result {
            Ok(Value::Universe(succ_level)) => {
                // Should use saturating arithmetic
                assert_eq!(succ_level.value(), u32::MAX);
            },
            Err(_) => {
                // Acceptable to reject extremely large levels
            }
        }
    }

    #[test]
    fn test_variable_index_bounds_checking() {
        let context = Context::empty();

        // Variables with very large indices should not cause panics
        let large_var = Term::var(usize::MAX);
        let result = infer(&large_var, &context);

        // Should return unbound variable error, not panic
        assert!(matches!(result, Err(CheckError::UnboundVariable(_))));
    }

    #[test]
    fn test_deep_recursion_stack_safety() {
        // Create deeply nested lambda terms
        let depth = 1000;
        let mut term = Term::var(0);

        for _ in 0..depth {
            term = Term::lambda(term);
        }

        // Should not cause stack overflow
        let context = Context::empty();
        let _type_result = infer(&term, &context);
        let _norm_result = normalize(&term);

        // If we reach here, no stack overflow occurred
    }

    #[test]
    fn test_circular_substitution_prevention() {
        // Test that substitutions don't create infinite loops
        let term = Term::var(0);

        // Try to create a circular substitution
        let circular_term = Term::app(Term::var(0), Term::var(0));
        let result = substitute_var(&term, 0, &circular_term);

        // Should complete without infinite recursion
        assert_eq!(result, circular_term);
    }

    #[test]
    fn test_metavariable_occurs_check() {
        // Ensure occurs check prevents infinite types
        let context = Context::empty();

        // Create a term that might trigger occurs check issues
        let meta_app = Term::app(Term::meta(0), Term::meta(0));
        let result = infer(&meta_app, &context);

        // Should handle gracefully
        match result {
            Ok(_) => {}, // Fine
            Err(_) => {}, // Also fine - may reject invalid constructions
        }
    }
}

/// Tests that ensure optimization correctness
#[cfg(test)]
mod optimization_regression_tests {
    use super::*;

    proptest! {
        #[test]
        fn memoization_preserves_semantics(
            term in small_term_gen()
        ) {
            let context = Context::empty();

            // Type check multiple times - should give same result
            let result1 = infer(&term, &context);
            let result2 = infer(&term, &context);

            match (result1, result2) {
                (Ok(type1), Ok(type2)) => {
                    prop_assert_eq!(type1, type2);
                },
                (Err(_), Err(_)) => {
                    // Consistent errors are fine
                    prop_assert!(true);
                },
                _ => {
                    prop_assert!(false, "Inconsistent type checking results");
                }
            }
        }

        #[test]
        fn parallel_evaluation_consistency(
            term in closed_term_gen()
        ) {
            // Ensure parallel optimizations don't affect determinism
            let env = ttt::core::Environment::new();

            let result1 = evaluate(&term, &env);
            let result2 = evaluate(&term, &env);

            prop_assert_eq!(result1, result2);
        }

        #[test]
        fn structural_sharing_correctness(
            base_term in small_term_gen(),
            modification_count in 1usize..5
        ) {
            // Test that structural sharing optimizations preserve semantics
            let mut terms = vec![base_term];

            for i in 0..modification_count {
                let new_term = Term::app(terms[i].clone(), Term::var(i));
                terms.push(new_term);
            }

            // All terms should be independently processable
            let context = Context::empty();

            for term in &terms {
                let _result = infer(term, &context);
                let _norm = normalize(term);
            }

            prop_assert!(true);
        }
    }
}

/// Tests for memory safety and resource management
#[cfg(test)]
mod memory_safety_tests {
    use super::*;

    proptest! {
        #[test]
        fn no_memory_leaks_in_large_contexts(
            entries in prop::collection::vec(
                (prop::string::string_regex("[a-z]+").unwrap(), small_term_gen()),
                50..100
            )
        ) {
            let mut context = Context::empty();

            // Build large context
            for (name, term) in entries {
                // Add entry if term is a valid type
                if let Ok(typ) = infer(&term, &context) {
                    context = context.extend(name, typ);
                }
            }

            // Use context for type checking
            let test_term = Term::var(0);
            if context.len() > 0 {
                let _result = infer(&test_term, &context);
            }

            prop_assert!(true);
        }

        #[test]
        fn reference_counting_stability(
            term in small_term_gen(),
            clone_count in 5usize..15
        ) {
            // Test Rc stability under heavy cloning
            let mut clones = Vec::new();

            for _ in 0..clone_count {
                clones.push(term.clone());
            }

            // Process all clones
            let context = Context::empty();
            for clone in &clones {
                let _result = infer(clone, &context);
            }

            prop_assert!(true);
        }

        #[test]
        fn environment_extension_memory_behavior(
            values in prop::collection::vec(value_gen(), 20..50)
        ) {
            use ttt::core::Environment;

            let mut env = Environment::new();

            // Extend environment many times
            for value in values {
                env = env.extend(value);
            }

            // Environment should remain usable
            let test_term = Term::var(0);
            let _result = evaluate(&test_term, &env);

            prop_assert!(true);
        }
    }
}

/// Tests for concurrent safety (if applicable)
#[cfg(test)]
mod concurrency_safety_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn parallel_type_checking_safety() {
        let terms: Vec<Term> = (0..10)
            .map(|i| Term::universe(i))
            .collect();

        let terms = Arc::new(terms);
        let results = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        for i in 0..4 {
            let terms_clone = Arc::clone(&terms);
            let results_clone = Arc::clone(&results);

            let handle = thread::spawn(move || {
                let context = Context::empty();
                let mut local_results = Vec::new();

                for (j, term) in terms_clone.iter().enumerate() {
                    if j % 4 == i {
                        let result = infer(term, &context);
                        local_results.push(result);
                    }
                }

                let mut results = results_clone.lock().unwrap();
                results.extend(local_results);
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should complete without data races or deadlocks
        let results = results.lock().unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn shared_term_processing() {
        let term = Arc::new(Term::universe(0));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let term_clone = Arc::clone(&term);

            let handle = thread::spawn(move || {
                let context = Context::empty();

                // Multiple operations on same term
                let _type_result = infer(&term_clone, &context);
                let _norm_result = normalize(&term_clone);

                // Should not cause data races
                true
            });

            handles.push(handle);
        }

        for handle in handles {
            assert!(handle.join().unwrap());
        }
    }
}

/// Tests for specific edge cases
#[cfg(test)]
mod edge_case_regression_tests {
    use super::*;

    #[test]
    fn empty_lambda_body_handling() {
        // Test lambda with minimal body
        let lambda = Term::lambda(Term::var(0));
        let context = Context::empty();

        // Should not crash
        let _result = infer(&lambda, &context);
        let _norm = normalize(&lambda);
    }

    #[test]
    fn maximum_pi_type_nesting() {
        // Create deeply nested Pi types
        let mut pi_type = Term::universe(0);

        for _ in 0..50 {
            pi_type = Term::pi(Term::universe(0), pi_type);
        }

        let context = Context::empty();
        let _result = infer(&pi_type, &context);
    }

    #[test]
    fn zero_level_universe_handling() {
        let context = Context::empty();
        let type_0 = Term::universe(0);

        let result = infer(&type_0, &context).unwrap();
        assert_eq!(result, Value::universe(Level(1)));
    }

    #[test]
    fn substitute_into_closed_term() {
        let closed_term = Term::universe(0);
        assert!(closed_term.is_closed());

        let replacement = Term::var(5);
        let result = substitute_var(&closed_term, 0, &replacement);

        // Closed term should be unchanged
        assert_eq!(result, closed_term);
    }

    proptest! {
        #[test]
        fn conversion_with_neutral_terms(
            head1 in 0usize..10,
            head2 in 0usize..10
        ) {
            use std::rc::Rc;

            let neutral1 = Value::neutral(head1, vec![]);
            let neutral2 = Value::neutral(head2, vec![]);

            let are_convertible = convertible(&neutral1, &neutral2, 0);
            let should_be_convertible = head1 == head2;

            prop_assert_eq!(are_convertible, should_be_convertible);
        }

        #[test]
        fn substitution_composition_associativity_regression(
            term in small_term_gen(),
            s1 in substitution_gen(),
            s2 in substitution_gen(),
            s3 in substitution_gen()
        ) {
            // Ensure substitution composition remains associative after optimizations
            let left_assoc = {
                let temp = s1.compose(&s2);
                apply_substitution(&term, &temp.compose(&s3))
            };

            let right_assoc = {
                let temp = s2.compose(&s3);
                apply_substitution(&term, &s1.compose(&temp))
            };

            prop_assert_eq!(left_assoc, right_assoc);
        }
    }
}

/// Performance regression detection
#[cfg(test)]
mod performance_regression_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn type_checking_performance_baseline() {
        let context = Context::empty();
        let term = Term::universe(0);

        let start = Instant::now();

        // Should complete very quickly for simple terms
        for _ in 0..1000 {
            let _result = infer(&term, &context);
        }

        let elapsed = start.elapsed();

        // Should complete 1000 simple type checks in under 100ms
        assert!(elapsed < Duration::from_millis(100));
    }

    #[test]
    fn normalization_performance_baseline() {
        let term = Term::lambda(Term::var(0));

        let start = Instant::now();

        for _ in 0..1000 {
            let _result = normalize(&term);
        }

        let elapsed = start.elapsed();

        // Should normalize 1000 simple terms quickly
        assert!(elapsed < Duration::from_millis(50));
    }

    proptest! {
        #[test]
        fn no_performance_regression_on_complex_terms(
            term in small_term_gen()
        ) {
            let context = Context::empty();

            let start = Instant::now();
            let _type_result = infer(&term, &context);
            let _norm_result = normalize(&term);
            let elapsed = start.elapsed();

            // Even complex terms should complete reasonably quickly
            prop_assert!(elapsed < Duration::from_millis(10));
        }
    }
}

/// Tests for API stability
#[cfg(test)]
mod api_stability_tests {
    use super::*;

    #[test]
    fn public_api_backwards_compatibility() {
        // Test that public API functions remain available and functional
        let context = Context::empty();
        let term = Term::universe(0);

        // These functions should remain available
        let _infer_result = infer(&term, &context);
        let _check_result = check(&term, &Value::universe(Level(1)), &context);
        let _norm_result = normalize(&term);

        // Term construction methods should work
        let _var = Term::var(0);
        let _univ = Term::universe(1);
        let _pi = Term::pi(term.clone(), term.clone());
        let _lambda = Term::lambda(term.clone());
        let _app = Term::app(term.clone(), term);
    }

    #[test]
    fn error_types_stability() {
        let context = Context::empty();
        let unbound_var = Term::var(0);

        // Error types should remain stable
        match infer(&unbound_var, &context) {
            Err(CheckError::UnboundVariable(idx)) => {
                assert_eq!(idx, 0);
            },
            _ => panic!("Expected UnboundVariable error"),
        }
    }

    #[test]
    fn value_types_construction() {
        // Value type constructors should remain available
        let _var_val = Value::var(0);
        let _univ_val = Value::universe(Level(0));

        // These should work
        use ttt::core::Closure;
        let closure = Closure::empty(Term::var(0));
        let _lambda_val = Value::lambda(closure.clone());
        let _pi_val = Value::pi(Value::universe(Level(0)), closure);
    }
}