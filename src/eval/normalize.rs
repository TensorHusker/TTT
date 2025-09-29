//! Term normalization and evaluation
//!
//! This module implements the core evaluation algorithm that converts
//! terms to their semantic values using normalization by evaluation.

use crate::core::{Term, Value, Environment, Closure};
use super::EvalResult;

/// Normalize a closed term to its canonical form
///
/// This is the main entry point for normalization. It evaluates
/// the term in an empty environment and then quotes the result
/// back to a normal term.
pub fn normalize(term: &Term) -> EvalResult<Term> {
    let env = Environment::new();
    let value = evaluate(term, &env)?;
    Ok(super::quote(&value, 0))
}

/// Normalize a term in a given environment
///
/// Used when normalizing open terms that contain free variables
/// bound in the surrounding context.
pub fn normalize_in_env(term: &Term, env: &Environment) -> EvalResult<Term> {
    let value = evaluate(term, env)?;
    Ok(super::quote(&value, env.len()))
}

/// Evaluate to weak-head normal form only
///
/// More efficient than full normalization when only the head
/// constructor is needed (e.g., for type checking applications).
pub fn weak_head_normalize(term: &Term, env: &Environment) -> EvalResult<Value> {
    evaluate(term, env)
}

/// Evaluate a term to a value in the given environment
///
/// This is the core evaluation function that implements the
/// semantic interpretation of dependent type theory terms.
pub fn evaluate(term: &Term, env: &Environment) -> EvalResult<Value> {
    match term {
        Term::Var(index) => {
            // Look up variable in environment
            if let Some(value) = env.lookup(*index) {
                Ok(value.clone())
            } else {
                // Free variable - convert index to level
                let level = env.len() + index;
                Ok(Value::var(level))
            }
        },

        Term::Universe(level) => {
            Ok(Value::universe(level.clone()))
        },

        Term::Pi(domain, codomain) => {
            let domain_val = evaluate(domain, env)?;
            let closure = Closure::new(env.clone(), codomain.as_ref().clone());
            Ok(Value::pi(domain_val, closure))
        },

        Term::Lambda(body) => {
            let closure = Closure::new(env.clone(), body.as_ref().clone());
            Ok(Value::lambda(closure))
        },

        Term::App(function, argument) => {
            let function_val = evaluate(function, env)?;
            let argument_val = evaluate(argument, env)?;
            super::apply_value(function_val, argument_val)
        },

        Term::Let(binding, body) => {
            // Evaluate let as (λx.body) binding
            let binding_val = evaluate(binding, env)?;
            let extended_env = env.extend(binding_val);
            evaluate(body, &extended_env)
        },

        Term::Meta(id) => {
            // Metavariables remain as neutral terms during evaluation
            // They will be resolved during unification
            Ok(Value::neutral(*id, Vec::new()))
        },
    }
}

/// Force evaluation of a value (eliminate lazy closures)
///
/// This function ensures that a value is fully evaluated, which
/// is necessary for certain operations like convertibility checking.
pub fn force_value(value: &Value) -> EvalResult<Value> {
    match value {
        Value::Var(_) | Value::Universe(_) | Value::Neutral(_) => {
            Ok(value.clone())
        },
        Value::Pi(domain, codomain) => {
            let forced_domain = force_value(domain)?;
            // Note: we don't force the codomain closure as it may contain free variables
            Ok(Value::pi(forced_domain, codomain.clone()))
        },
        Value::Lambda(_) => {
            // Lambdas are already in normal form
            Ok(value.clone())
        },
    }
}

/// Unfold a closure by applying it to a fresh variable
///
/// Used during convertibility checking to compare function bodies.
/// The fresh variable must have a level higher than any existing variable.
pub fn unfold_closure(closure: &Closure, var_level: usize) -> EvalResult<Value> {
    let var = Value::var(var_level);
    super::instantiate_closure(closure, var)
}

/// Check if a value is in normal form
///
/// A value is in normal form if it contains no reducible expressions.
/// This is always true for values in our implementation since evaluation
/// produces canonical forms.
pub fn is_normal(value: &Value) -> bool {
    match value {
        Value::Var(_) | Value::Universe(_) => true,
        Value::Pi(domain, _) => is_normal(domain),
        Value::Lambda(_) => true, // Closures are normal by construction
        Value::Neutral(_) => true, // Neutral terms are stuck, hence normal
    }
}

/// Compute the normal form of a term for debugging
///
/// Returns both the evaluated value and its quoted normal form.
/// Useful for understanding the evaluation process.
pub fn debug_normalize(term: &Term) -> EvalResult<(Value, Term)> {
    let env = Environment::new();
    let value = evaluate(term, &env)?;
    let normal_term = super::quote(&value, 0);
    Ok((value, normal_term))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Term, Level};

    #[test]
    fn test_evaluate_variable() {
        let env = Environment::new().extend(Value::universe(Level::TYPE));
        let term = Term::var(0);
        let result = evaluate(&term, &env).unwrap();
        assert_eq!(result, Value::universe(Level::TYPE));
    }

    #[test]
    fn test_evaluate_universe() {
        let env = Environment::new();
        let term = Term::universe(1);
        let result = evaluate(&term, &env).unwrap();
        assert_eq!(result, Value::universe(Level(1)));
    }

    #[test]
    fn test_evaluate_lambda() {
        let env = Environment::new();
        let term = Term::lambda(Term::var(0)); // λx.x
        let result = evaluate(&term, &env).unwrap();

        match result {
            Value::Lambda(closure) => {
                assert_eq!(closure.env, env);
                assert_eq!(*closure.term, Term::var(0));
            },
            _ => panic!("Expected lambda value"),
        }
    }

    #[test]
    fn test_evaluate_application() {
        let env = Environment::new();
        // (λx.x) Type₀
        let identity = Term::lambda(Term::var(0));
        let arg = Term::universe(0);
        let app = Term::app(identity, arg.clone());

        let result = evaluate(&app, &env).unwrap();
        assert_eq!(result, Value::universe(Level::TYPE));
    }

    #[test]
    fn test_normalize_identity() {
        // λx.x should normalize to itself
        let term = Term::lambda(Term::var(0));
        let normalized = normalize(&term).unwrap();
        assert_eq!(normalized, term);
    }

    #[test]
    fn test_normalize_beta_reduction() {
        // (λx.x) Type₀ should normalize to Type₀
        let identity = Term::lambda(Term::var(0));
        let arg = Term::universe(0);
        let app = Term::app(identity, arg.clone());

        let normalized = normalize(&app).unwrap();
        assert_eq!(normalized, arg);
    }

    #[test]
    fn test_let_evaluation() {
        // let x = Type₀ in x should evaluate to Type₀
        let binding = Term::universe(0);
        let body = Term::var(0);
        let let_term = Term::let_in(binding.clone(), body);

        let env = Environment::new();
        let result = evaluate(&let_term, &env).unwrap();
        assert_eq!(result, Value::universe(Level::TYPE));
    }

    #[test]
    fn test_free_variable() {
        // Variable with no binding should become neutral
        let env = Environment::new();
        let term = Term::var(0);
        let result = evaluate(&term, &env).unwrap();
        assert_eq!(result, Value::var(0));
    }

    #[test]
    fn test_force_value() {
        let val = Value::universe(Level::TYPE);
        let forced = force_value(&val).unwrap();
        assert_eq!(forced, val);
    }

    #[test]
    fn test_is_normal() {
        assert!(is_normal(&Value::var(0)));
        assert!(is_normal(&Value::universe(Level::TYPE)));

        let closure = Closure::empty(Term::var(0));
        assert!(is_normal(&Value::lambda(closure)));
    }
}