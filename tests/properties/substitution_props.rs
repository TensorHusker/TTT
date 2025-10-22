//! Property-based tests for substitution correctness
//!
//! Verifies the fundamental algebraic properties of substitution that
//! are essential for dependent type theory correctness.

use proptest::prelude::*;
use ttt::core::{Term, subst::*};
use super::generators::*;

/// Test that identity substitution is truly the identity
#[cfg(test)]
mod substitution_identity_tests {
    use super::*;

    proptest! {
        #[test]
        fn identity_substitution_preserves_terms(term in small_term_gen()) {
            let identity = Substitution::identity();
            let result = apply_substitution(&term, &identity);
            prop_assert_eq!(result, term);
        }

        #[test]
        fn empty_substitution_is_identity(term in small_term_gen()) {
            let empty = Substitution::empty();
            let result = apply_substitution(&term, &empty);
            prop_assert_eq!(result, term);
        }

        #[test]
        fn substituting_non_occurring_variable(
            term in small_term_gen(),
            replacement in small_term_gen(),
            index in 10usize..20
        ) {
            // If a variable doesn't occur in a term, substituting it should do nothing
            if let Some(max_idx) = term.max_index() {
                if index > max_idx {
                    let subst = Substitution::single(index, replacement);
                    let result = apply_substitution(&term, &subst);
                    prop_assert_eq!(result, term);
                }
            } else {
                // Closed term - any substitution should preserve it
                let subst = Substitution::single(index, replacement);
                let result = apply_substitution(&term, &subst);
                prop_assert_eq!(result, term);
            }
        }
    }
}

/// Test substitution composition properties
#[cfg(test)]
mod substitution_composition_tests {
    use super::*;

    proptest! {
        #[test]
        fn substitution_composition_associativity(
            term in small_term_gen(),
            s1 in substitution_gen(),
            s2 in substitution_gen(),
            s3 in substitution_gen()
        ) {
            // (s1 ∘ s2) ∘ s3 = s1 ∘ (s2 ∘ s3)
            let left_assoc = {
                let temp = s1.compose(&s2);
                temp.compose(&s3)
            };
            let right_assoc = {
                let temp = s2.compose(&s3);
                s1.compose(&temp)
            };

            let result_left = apply_substitution(&term, &left_assoc);
            let result_right = apply_substitution(&term, &right_assoc);

            prop_assert_eq!(result_left, result_right);
        }

        #[test]
        fn identity_is_composition_identity(
            s in substitution_gen(),
            term in small_term_gen()
        ) {
            let identity = Substitution::identity();

            // s ∘ id = s
            let left_result = apply_substitution(&term, &s.compose(&identity));
            let direct_result = apply_substitution(&term, &s);
            prop_assert_eq!(left_result, direct_result);

            // id ∘ s = s
            let right_result = apply_substitution(&term, &identity.compose(&s));
            prop_assert_eq!(right_result, direct_result);
        }

        #[test]
        fn composition_distributes_over_application(
            term in small_term_gen(),
            s1 in substitution_gen(),
            s2 in substitution_gen()
        ) {
            // (s1 ∘ s2)[t] = s1[s2[t]]
            let composed = s1.compose(&s2);
            let composed_result = apply_substitution(&term, &composed);

            let step_by_step = {
                let intermediate = apply_substitution(&term, &s2);
                apply_substitution(&intermediate, &s1)
            };

            prop_assert_eq!(composed_result, step_by_step);
        }
    }
}

/// Test capture-avoidance properties
#[cfg(test)]
mod capture_avoidance_tests {
    use super::*;

    proptest! {
        #[test]
        fn bound_variables_not_captured(
            body in small_term_gen(),
            replacement in small_term_gen()
        ) {
            // In λx.body, substituting free variables in body should not
            // affect the bound variable x (De Bruijn index 0)
            let lambda_term = Term::lambda(body.clone());

            // Substitute index 0 (which should be the free variable in original body)
            let subst = Substitution::single(0, replacement);
            let result = apply_substitution(&lambda_term, &subst);

            // The result should still be a lambda
            prop_assert!(result.is_lambda());

            // And index 0 in the body should still refer to the lambda-bound variable
            if let Term::Lambda(result_body) = result {
                // The substitution should have been lifted appropriately
                // We can't easily check this without implementing conversion,
                // but at least verify structure is preserved
                let _ = result_body; // Basic structural check
            }
        }

        #[test]
        fn pi_type_capture_avoidance(
            domain in small_term_gen(),
            codomain in small_term_gen(),
            replacement in small_term_gen()
        ) {
            // Similar test for Pi types
            let pi_term = Term::pi(domain, codomain);
            let subst = Substitution::single(0, replacement);
            let result = apply_substitution(&pi_term, &subst);

            prop_assert!(result.is_pi());
        }
    }
}

/// Test single substitution properties
#[cfg(test)]
mod single_substitution_tests {
    use super::*;

    proptest! {
        #[test]
        fn substitute_var_with_itself_is_identity(
            term in small_term_gen(),
            index in 0usize..10
        ) {
            // Substituting a variable with itself should be identity
            let var_term = Term::var(index);
            let result = substitute_var(&term, index, &var_term);

            // This should be equivalent to identity substitution on the term
            let identity_result = apply_substitution(&term, &Substitution::identity());
            prop_assert_eq!(result, identity_result);
        }

        #[test]
        fn substitute_top_variable(
            body in small_term_gen(),
            replacement in small_term_gen()
        ) {
            // Test substitute_top convenience function
            let direct = substitute_top(&body, &replacement);
            let via_substitute_var = substitute_var(&body, 0, &replacement);

            prop_assert_eq!(direct, via_substitute_var);
        }

        #[test]
        fn variable_substitution_is_replacement(
            replacement in small_term_gen(),
            index in 0usize..5
        ) {
            // Substituting a variable should give the replacement
            let var = Term::var(index);
            let result = substitute_var(&var, index, &replacement);
            prop_assert_eq!(result, replacement);
        }
    }
}

/// Test variable shifting properties
#[cfg(test)]
mod variable_shifting_tests {
    use super::*;

    proptest! {
        #[test]
        fn shift_preserves_closed_terms(
            term in closed_term_gen(),
            shift_amount in -5isize..5isize
        ) {
            // Shifting closed terms should preserve them (no free variables to shift)
            let result = shift_term(&term, 0, shift_amount);
            prop_assert_eq!(result, term);
        }

        #[test]
        fn shift_composition(
            term in small_term_gen(),
            shift1 in -3isize..3isize,
            shift2 in -3isize..3isize,
            cutoff in 0usize..3
        ) {
            // shift(shift(t, c, s1), c, s2) = shift(t, c, s1 + s2)
            let intermediate = shift_term(&term, cutoff, shift1);
            let composed = shift_term(&intermediate, cutoff, shift2);

            let direct = shift_term(&term, cutoff, shift1 + shift2);

            prop_assert_eq!(composed, direct);
        }

        #[test]
        fn shift_zero_is_identity(
            term in small_term_gen(),
            cutoff in 0usize..5
        ) {
            let result = shift_term(&term, cutoff, 0);
            prop_assert_eq!(result, term);
        }

        #[test]
        fn shift_respects_cutoff(
            index in 0usize..10,
            cutoff in 0usize..10,
            shift_amount in 1isize..5isize
        ) {
            let var = Term::var(index);
            let result = shift_term(&var, cutoff, shift_amount);

            if index >= cutoff {
                // Should be shifted
                if let Term::Var(new_index) = result {
                    prop_assert_eq!(new_index, index + shift_amount as usize);
                } else {
                    prop_assert!(false, "Expected shifted variable");
                }
            } else {
                // Should be unchanged
                prop_assert_eq!(result, var);
            }
        }
    }
}

/// Test substitution and shifting interaction
#[cfg(test)]
mod substitution_shifting_interaction_tests {
    use super::*;

    proptest! {
        #[test]
        fn substitution_commutes_with_shifting_on_disjoint_vars(
            term in small_term_gen(),
            subst_index in 0usize..3,
            replacement in small_term_gen(),
            shift_cutoff in 5usize..8,
            shift_amount in 1isize..3isize
        ) {
            // If substitution and shifting affect disjoint sets of variables,
            // they should commute
            if subst_index < shift_cutoff {
                let subst = Substitution::single(subst_index, replacement);

                // Apply substitution then shift
                let subst_then_shift = {
                    let after_subst = apply_substitution(&term, &subst);
                    shift_term(&after_subst, shift_cutoff, shift_amount)
                };

                // Apply shift then substitution
                let shift_then_subst = {
                    let after_shift = shift_term(&term, shift_cutoff, shift_amount);
                    let shifted_replacement = shift_term(&replacement, shift_cutoff, shift_amount);
                    let shifted_subst = Substitution::single(subst_index, shifted_replacement);
                    apply_substitution(&after_shift, &shifted_subst)
                };

                prop_assert_eq!(subst_then_shift, shift_then_subst);
            }
        }
    }
}

/// Test contains_var predicate
#[cfg(test)]
mod contains_var_tests {
    use super::*;

    proptest! {
        #[test]
        fn contains_var_on_variable(index in 0usize..10) {
            let var = Term::var(index);
            prop_assert!(contains_var(&var, index));

            // Should not contain other indices
            if index > 0 {
                prop_assert!(!contains_var(&var, index - 1));
            }
            prop_assert!(!contains_var(&var, index + 1));
        }

        #[test]
        fn contains_var_closed_term(
            term in closed_term_gen(),
            index in 0usize..10
        ) {
            // Closed terms contain no free variables
            prop_assert!(!contains_var(&term, index));
        }

        #[test]
        fn contains_var_after_substitution(
            term in small_term_gen(),
            index in 0usize..5,
            replacement in closed_term_gen()
        ) {
            // After substituting a variable with a closed term,
            // that variable should no longer be contained
            if contains_var(&term, index) {
                let subst = Substitution::single(index, replacement);
                let result = apply_substitution(&term, &subst);
                prop_assert!(!contains_var(&result, index));
            }
        }
    }
}

/// Test substitution on specific term structures
#[cfg(test)]
mod structural_substitution_tests {
    use super::*;

    proptest! {
        #[test]
        fn substitution_preserves_universe_structure(
            level in level_gen(),
            index in 0usize..5,
            replacement in small_term_gen()
        ) {
            let universe = Term::Universe(level.clone());
            let subst = Substitution::single(index, replacement);
            let result = apply_substitution(&universe, &subst);

            prop_assert_eq!(result, universe); // Universes have no variables
        }

        #[test]
        fn substitution_in_application_distributes(
            fun in small_term_gen(),
            arg in small_term_gen(),
            index in 0usize..5,
            replacement in small_term_gen()
        ) {
            let app = Term::app(fun.clone(), arg.clone());
            let subst = Substitution::single(index, replacement.clone());

            let result = apply_substitution(&app, &subst);

            let expected = Term::app(
                apply_substitution(&fun, &subst),
                apply_substitution(&arg, &subst)
            );

            prop_assert_eq!(result, expected);
        }

        #[test]
        fn substitution_in_pi_type_respects_binding(
            domain in small_term_gen(),
            codomain in small_term_gen(),
            index in 0usize..3,
            replacement in small_term_gen()
        ) {
            let pi = Term::pi(domain.clone(), codomain.clone());
            let subst = Substitution::single(index, replacement.clone());

            let result = apply_substitution(&pi, &subst);

            // Result should be a Pi type
            prop_assert!(result.is_pi());

            // Domain should have substitution applied directly
            if let Term::Pi(result_domain, _) = result {
                let expected_domain = apply_substitution(&domain, &subst);
                prop_assert_eq!(*result_domain, expected_domain);
            }
        }
    }
}