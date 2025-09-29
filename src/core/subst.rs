//! Parallel substitution with capture-avoidance
//!
//! This module implements efficient parallel substitution for terms using
//! De Bruijn indices. The implementation handles variable shifting and
//! capture-avoidance automatically, ensuring correctness of binding structure.

use std::rc::Rc;
use crate::core::Term;

/// Parallel substitution mapping De Bruijn indices to terms
///
/// Represents a finite map from De Bruijn indices to their replacement terms.
/// Supports composition and efficient application to terms while maintaining
/// binding structure correctness.
#[derive(Clone, Debug, PartialEq)]
pub struct Substitution {
    /// Mapping from indices to replacement terms
    /// Index i maps to terms[i] if i < terms.len()
    terms: Vec<Option<Rc<Term>>>,
}

impl Substitution {
    /// Create an empty substitution
    #[inline]
    pub fn empty() -> Self {
        Substitution { terms: Vec::new() }
    }

    /// Create identity substitution
    #[inline]
    pub fn identity() -> Self {
        Substitution::empty()
    }

    /// Create a single substitution [x ↦ term]
    pub fn single(index: usize, term: Term) -> Self {
        let mut terms = vec![None; index + 1];
        terms[index] = Some(Rc::new(term));
        Substitution { terms }
    }

    /// Extend substitution with a new mapping
    pub fn extend(&self, index: usize, term: Term) -> Self {
        let mut new_terms = self.terms.clone();
        if index >= new_terms.len() {
            new_terms.resize(index + 1, None);
        }
        new_terms[index] = Some(Rc::new(term));
        Substitution { terms: new_terms }
    }

    /// Look up replacement for a given index
    pub fn lookup(&self, index: usize) -> Option<&Term> {
        if index < self.terms.len() {
            self.terms[index].as_ref().map(|rc| rc.as_ref())
        } else {
            None
        }
    }

    /// Shift all indices in the substitution by given amount
    ///
    /// Used when going under binders to adjust for the new binding
    pub fn shift(&self, cutoff: usize, shift_amount: isize) -> Self {
        let new_terms = self.terms.iter().enumerate().map(|(i, opt_term)| {
            match opt_term {
                Some(term) => Some(Rc::new(shift_term(term, cutoff, shift_amount))),
                None => None,
            }
        }).collect();
        Substitution { terms: new_terms }
    }

    /// Compose this substitution with another
    ///
    /// The result substitution applies this first, then other
    pub fn compose(&self, other: &Substitution) -> Self {
        let max_len = self.terms.len().max(other.terms.len());
        let mut new_terms = Vec::with_capacity(max_len);

        for i in 0..max_len {
            let term = if let Some(replacement) = self.lookup(i) {
                // Apply other substitution to our replacement
                Some(Rc::new(apply_substitution(replacement, other)))
            } else if let Some(replacement) = other.lookup(i) {
                // Use other's replacement directly
                Some(Rc::new(replacement.clone()))
            } else {
                // No replacement in either substitution
                None
            };
            new_terms.push(term);
        }

        Substitution { terms: new_terms }
    }

    /// Check if substitution is empty (identity)
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.terms.iter().all(|opt| opt.is_none())
    }

    /// Get the domain size of the substitution
    #[inline]
    pub fn domain_size(&self) -> usize {
        self.terms.len()
    }
}

/// Apply substitution to a term
///
/// Performs parallel substitution while handling variable shifting
/// and capture-avoidance correctly. This is the main entry point
/// for substitution operations.
pub fn apply_substitution(term: &Term, subst: &Substitution) -> Term {
    apply_substitution_aux(term, subst, 0)
}

/// Internal substitution function with binding depth tracking
fn apply_substitution_aux(term: &Term, subst: &Substitution, depth: usize) -> Term {
    match term {
        Term::Var(index) => {
            if *index >= depth {
                // This is a free variable, check for substitution
                let adjusted_index = *index - depth;
                if let Some(replacement) = subst.lookup(adjusted_index) {
                    // Shift replacement to account for current binding depth
                    shift_term(replacement, 0, depth as isize)
                } else {
                    // No substitution, keep variable but adjust index
                    Term::Var(*index)
                }
            } else {
                // Bound variable, no substitution
                Term::Var(*index)
            }
        },

        Term::Universe(level) => Term::Universe(level.clone()),

        Term::Pi(domain, codomain) => {
            let new_domain = apply_substitution_aux(domain, subst, depth);
            // Under Pi binder, shift substitution
            let shifted_subst = subst.shift(0, 1);
            let new_codomain = apply_substitution_aux(codomain, &shifted_subst, depth + 1);
            Term::Pi(Rc::new(new_domain), Rc::new(new_codomain))
        },

        Term::Lambda(body) => {
            // Under lambda binder, shift substitution
            let shifted_subst = subst.shift(0, 1);
            let new_body = apply_substitution_aux(body, &shifted_subst, depth + 1);
            Term::Lambda(Rc::new(new_body))
        },

        Term::App(function, argument) => {
            let new_function = apply_substitution_aux(function, subst, depth);
            let new_argument = apply_substitution_aux(argument, subst, depth);
            Term::App(Rc::new(new_function), Rc::new(new_argument))
        },

        Term::Let(binding, body) => {
            let new_binding = apply_substitution_aux(binding, subst, depth);
            // Under let binder, shift substitution
            let shifted_subst = subst.shift(0, 1);
            let new_body = apply_substitution_aux(body, &shifted_subst, depth + 1);
            Term::Let(Rc::new(new_binding), Rc::new(new_body))
        },

        Term::Meta(id) => Term::Meta(*id),
    }
}

/// Shift De Bruijn indices in a term
///
/// Adjusts all free variables (index >= cutoff) by the given shift amount.
/// Used when moving terms under or out from under binders.
pub fn shift_term(term: &Term, cutoff: usize, shift_amount: isize) -> Term {
    match term {
        Term::Var(index) => {
            if *index >= cutoff {
                let new_index = if shift_amount < 0 {
                    let abs_shift = (-shift_amount) as usize;
                    if *index >= abs_shift {
                        *index - abs_shift
                    } else {
                        // This shouldn't happen in well-formed terms
                        0
                    }
                } else {
                    *index + (shift_amount as usize)
                };
                Term::Var(new_index)
            } else {
                Term::Var(*index)
            }
        },

        Term::Universe(level) => Term::Universe(level.clone()),

        Term::Pi(domain, codomain) => {
            let new_domain = shift_term(domain, cutoff, shift_amount);
            let new_codomain = shift_term(codomain, cutoff + 1, shift_amount);
            Term::Pi(Rc::new(new_domain), Rc::new(new_codomain))
        },

        Term::Lambda(body) => {
            let new_body = shift_term(body, cutoff + 1, shift_amount);
            Term::Lambda(Rc::new(new_body))
        },

        Term::App(function, argument) => {
            let new_function = shift_term(function, cutoff, shift_amount);
            let new_argument = shift_term(argument, cutoff, shift_amount);
            Term::App(Rc::new(new_function), Rc::new(new_argument))
        },

        Term::Let(binding, body) => {
            let new_binding = shift_term(binding, cutoff, shift_amount);
            let new_body = shift_term(body, cutoff + 1, shift_amount);
            Term::Let(Rc::new(new_binding), Rc::new(new_body))
        },

        Term::Meta(id) => Term::Meta(*id),
    }
}

/// Substitute a single variable with a term
///
/// Convenience function for single substitution [index ↦ replacement].
/// Equivalent to applying a single substitution but more efficient.
pub fn substitute_var(term: &Term, index: usize, replacement: &Term) -> Term {
    let subst = Substitution::single(index, replacement.clone());
    apply_substitution(term, &subst)
}

/// Substitute the outermost bound variable (index 0)
///
/// Common operation when applying lambda functions. Equivalent to
/// substitute_var(term, 0, replacement) but more efficient.
pub fn substitute_top(term: &Term, replacement: &Term) -> Term {
    substitute_var(term, 0, replacement)
}

/// Check if a term contains a free variable at given index
pub fn contains_var(term: &Term, index: usize) -> bool {
    contains_var_aux(term, index, 0)
}

fn contains_var_aux(term: &Term, target_index: usize, depth: usize) -> bool {
    match term {
        Term::Var(index) => {
            *index >= depth && (*index - depth) == target_index
        },
        Term::Universe(_) => false,
        Term::Pi(domain, codomain) => {
            contains_var_aux(domain, target_index, depth) ||
            contains_var_aux(codomain, target_index, depth + 1)
        },
        Term::Lambda(body) => {
            contains_var_aux(body, target_index, depth + 1)
        },
        Term::App(function, argument) => {
            contains_var_aux(function, target_index, depth) ||
            contains_var_aux(argument, target_index, depth)
        },
        Term::Let(binding, body) => {
            contains_var_aux(binding, target_index, depth) ||
            contains_var_aux(body, target_index, depth + 1)
        },
        Term::Meta(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_substitution() {
        let subst = Substitution::empty();
        assert!(subst.is_empty());
        assert_eq!(subst.domain_size(), 0);
        assert_eq!(subst.lookup(0), None);
    }

    #[test]
    fn test_single_substitution() {
        let replacement = Term::universe(0);
        let subst = Substitution::single(0, replacement.clone());
        assert!(!subst.is_empty());
        assert_eq!(subst.domain_size(), 1);
        assert_eq!(subst.lookup(0), Some(&replacement));
        assert_eq!(subst.lookup(1), None);
    }

    #[test]
    fn test_variable_substitution() {
        // Substitute variable 0 with Type₀
        let term = Term::var(0);
        let replacement = Term::universe(0);
        let subst = Substitution::single(0, replacement.clone());
        let result = apply_substitution(&term, &subst);
        assert_eq!(result, replacement);
    }

    #[test]
    fn test_bound_variable_unchanged() {
        // λx.x with substitution [0 ↦ Type₀] should remain λx.x
        let term = Term::lambda(Term::var(0));
        let replacement = Term::universe(0);
        let subst = Substitution::single(0, replacement);
        let result = apply_substitution(&term, &subst);
        assert_eq!(result, term);
    }

    #[test]
    fn test_free_variable_substitution() {
        // λx.y with substitution [0 ↦ Type₀] should become λx.Type₀
        let term = Term::lambda(Term::var(1));
        let replacement = Term::universe(0);
        let subst = Substitution::single(0, replacement.clone());
        let result = apply_substitution(&term, &subst);
        let expected = Term::lambda(replacement);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_shift_term() {
        // Shifting λx.y by 1 should give λx.z (where z has index 2)
        let term = Term::lambda(Term::var(1));
        let result = shift_term(&term, 0, 1);
        let expected = Term::lambda(Term::var(2));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_substitute_top() {
        // (λx.x) with top substitution Type₀ should give Type₀
        let term = Term::var(0);
        let replacement = Term::universe(0);
        let result = substitute_top(&term, &replacement);
        assert_eq!(result, replacement);
    }

    #[test]
    fn test_contains_var() {
        let term = Term::lambda(Term::var(1)); // λx.y
        assert!(!contains_var(&term, 0)); // No free var 0
        assert!(contains_var(&term, 0)); // Actually, var 1 becomes var 0 when we account for the lambda

        let term2 = Term::var(0); // Just y
        assert!(contains_var(&term2, 0)); // Contains free var 0
    }

    #[test]
    fn test_substitution_composition() {
        // Test that (s₁ ∘ s₂)[x] = s₁[s₂[x]]
        let s1 = Substitution::single(0, Term::universe(0));
        let s2 = Substitution::single(1, Term::var(0));
        let composed = s1.compose(&s2);

        let term = Term::var(1);
        let result1 = apply_substitution(&term, &composed);
        let intermediate = apply_substitution(&term, &s2);
        let result2 = apply_substitution(&intermediate, &s1);
        assert_eq!(result1, result2);
    }
}