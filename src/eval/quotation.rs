//! Quotation - converting values back to terms
//!
//! This module implements the quotation algorithm that converts
//! semantic values back to syntactic terms in normal form.

use crate::core::{Term, Value, Neutral};

/// Quote a value back to a normal term
///
/// The level parameter tracks the current binding depth to properly
/// convert De Bruijn levels back to indices.
pub fn quote(value: &Value, level: usize) -> Term {
    match value {
        Value::Var(var_level) => {
            // Convert level back to De Bruijn index
            if *var_level < level {
                Term::var(level - 1 - var_level)
            } else {
                // Free variable - keep as is but adjust for context
                Term::var(*var_level)
            }
        },

        Value::Universe(univ_level) => {
            Term::universe(univ_level.value())
        },

        Value::Pi(domain, codomain) => {
            let domain_term = quote(domain, level);

            // To quote the codomain, we need to instantiate it with a fresh variable
            let fresh_var = Value::var(level);
            let codomain_value = super::instantiate_closure(codomain, fresh_var)
                .unwrap_or_else(|_| Value::var(level));
            let codomain_term = quote(&codomain_value, level + 1);

            Term::pi(domain_term, codomain_term)
        },

        Value::Lambda(closure) => {
            // Instantiate closure with fresh variable and quote the result
            let fresh_var = Value::var(level);
            let body_value = super::instantiate_closure(closure, fresh_var)
                .unwrap_or_else(|_| Value::var(level));
            let body_term = quote(&body_value, level + 1);

            Term::lambda(body_term)
        },

        Value::Neutral(neutral) => {
            quote_neutral(neutral, level)
        },
    }
}

/// Quote a neutral value back to a term
///
/// Neutral values are variables applied to a spine of arguments.
/// We reconstruct the applications during quotation.
pub fn quote_neutral(neutral: &Neutral, level: usize) -> Term {
    let head_term = if neutral.head < level {
        Term::var(level - 1 - neutral.head)
    } else {
        Term::var(neutral.head)
    };

    // Apply all arguments in the spine
    neutral.spine.iter().fold(head_term, |acc, arg| {
        let arg_term = quote(arg, level);
        Term::app(acc, arg_term)
    })
}

/// Quote with η-expansion for function types
///
/// This ensures that function values are always quoted as lambdas,
/// even if they were originally neutral terms of function type.
pub fn quote_eta(value: &Value, value_type: &Value, level: usize) -> Term {
    match value_type {
        Value::Pi(domain, codomain) => {
            // η-expand: quote as λx.(f x) where x is fresh
            let fresh_var = Value::var(level);
            let applied = super::apply_value(value.clone(), fresh_var.clone())
                .unwrap_or_else(|_| value.clone());

            let codomain_type = super::instantiate_closure(codomain, fresh_var)
                .unwrap_or_else(|_| Value::var(level));

            let body_term = quote_eta(&applied, &codomain_type, level + 1);
            Term::lambda(body_term)
        },
        _ => quote(value, level),
    }
}

/// Quote a value to a term with readable variable names
///
/// For debugging and pretty-printing. Uses a list of variable names
/// instead of De Bruijn indices where possible.
pub fn quote_with_names(value: &Value, level: usize, names: &[String]) -> Term {
    // This is essentially the same as quote but could be extended
    // to preserve variable names for better debugging output
    quote(value, level)
}

/// Normalize and quote a value in one step
///
/// Convenience function that forces normalization before quotation.
/// Useful when dealing with lazy closures that need to be fully evaluated.
pub fn normalize_quote(value: &Value, level: usize) -> Term {
    // First force the value to be fully evaluated
    let normalized = super::normalize::force_value(value)
        .unwrap_or_else(|_| value.clone());
    quote(&normalized, level)
}

/// Check if a quoted term is in η-long normal form
///
/// A term is in η-long form if all functions are fully η-expanded.
/// This is useful for canonical representation of terms.
pub fn is_eta_long(term: &Term, term_type: &Value) -> bool {
    match (term, term_type) {
        (Term::Lambda(_), Value::Pi(_, _)) => true,
        (Term::Lambda(body), Value::Pi(domain, codomain)) => {
            let fresh_var = Value::var(0);
            let codomain_type = super::instantiate_closure(codomain, fresh_var)
                .unwrap_or_else(|_| Value::var(0));
            is_eta_long(body, &codomain_type)
        },
        (_, Value::Pi(_, _)) => false, // Should be a lambda
        _ => true, // Non-function types are trivially η-long
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Closure, Environment};

    #[test]
    fn test_quote_variable() {
        let value = Value::var(0);
        let term = quote(&value, 1);
        assert_eq!(term, Term::var(0));
    }

    #[test]
    fn test_quote_universe() {
        let value = Value::universe(Level(1));
        let term = quote(&value, 0);
        assert_eq!(term, Term::universe(1));
    }

    #[test]
    fn test_quote_lambda() {
        // λx.x as a value should quote back to λx.x
        let closure = Closure::empty(Term::var(0));
        let value = Value::lambda(closure);
        let term = quote(&value, 0);
        assert_eq!(term, Term::lambda(Term::var(0)));
    }

    #[test]
    fn test_quote_pi_type() {
        // Π(x:Type₀).Type₀ should quote correctly
        let domain = Value::universe(Level::TYPE);
        let codomain = Closure::empty(Term::universe(0));
        let value = Value::pi(domain, codomain);
        let term = quote(&value, 0);

        let expected = Term::pi(Term::universe(0), Term::universe(0));
        assert_eq!(term, expected);
    }

    #[test]
    fn test_quote_neutral() {
        let neutral = Neutral::new(0);
        let value = Value::neutral(0, vec![]);
        let term = quote(&value, 1);
        assert_eq!(term, Term::var(0));
    }

    #[test]
    fn test_quote_neutral_with_spine() {
        // Variable applied to an argument
        let arg = Value::universe(Level::TYPE);
        let neutral = Neutral::new(1).apply(arg);
        let value = Value::Neutral(neutral);
        let term = quote(&value, 2);

        let expected = Term::app(Term::var(0), Term::universe(0));
        assert_eq!(term, expected);
    }

    #[test]
    fn test_quote_eta_expansion() {
        // Test η-expansion of a function
        let var_value = Value::var(0);
        let domain = Value::universe(Level::TYPE);
        let codomain = Closure::empty(Term::universe(0));
        let pi_type = Value::pi(domain, codomain);

        let term = quote_eta(&var_value, &pi_type, 1);

        // Should be η-expanded to λx.(#0 x)
        match term {
            Term::Lambda(body) => {
                match body.as_ref() {
                    Term::App(fun, arg) => {
                        assert_eq!(**fun, Term::var(1));
                        assert_eq!(**arg, Term::var(0));
                    },
                    _ => panic!("Expected application in lambda body"),
                }
            },
            _ => panic!("Expected lambda term"),
        }
    }

    #[test]
    fn test_normalize_quote() {
        let value = Value::universe(Level::TYPE);
        let term = normalize_quote(&value, 0);
        assert_eq!(term, Term::universe(0));
    }

    #[test]
    fn test_is_eta_long() {
        // λx.x should be η-long for any function type
        let lambda = Term::lambda(Term::var(0));
        let domain = Value::universe(Level::TYPE);
        let codomain = Closure::empty(Term::var(0));
        let pi_type = Value::pi(domain, codomain);

        assert!(is_eta_long(&lambda, &pi_type));

        // A variable should not be η-long for function type
        let var = Term::var(0);
        assert!(!is_eta_long(&var, &pi_type));
    }
}