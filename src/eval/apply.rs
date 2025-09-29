//! Function application and closure instantiation
//!
//! This module handles the core reduction operations for
//! function application and closure evaluation.

use crate::core::{Value, Closure, Neutral};
use super::{EvalError, EvalResult};

/// Apply a value to another value (function application)
///
/// This is the core reduction operation that handles beta-reduction
/// and neutral term construction when reduction is blocked.
pub fn apply_value(function: Value, argument: Value) -> EvalResult<Value> {
    match function {
        Value::Lambda(closure) => {
            // Beta reduction: (λx.e) v → e[x := v]
            instantiate_closure(&closure, argument)
        },

        Value::Neutral(neutral) => {
            // Can't reduce - extend the spine
            let applied = neutral.apply(argument);
            Ok(Value::Neutral(applied))
        },

        Value::Var(level) => {
            // Variable applied to argument - create neutral term
            let neutral = Neutral::new(level).apply(argument);
            Ok(Value::Neutral(neutral))
        },

        _ => {
            Err(EvalError::NotAFunction(function))
        }
    }
}

/// Instantiate a closure by substituting a value for the bound variable
///
/// This is the core operation for evaluating under binders. It extends
/// the closure's environment with the new value and evaluates the body.
pub fn instantiate_closure(closure: &Closure, value: Value) -> EvalResult<Value> {
    let extended_env = closure.env.extend(value);
    super::normalize::evaluate(&closure.term, &extended_env)
}

/// Apply multiple arguments to a value
///
/// Convenience function for applying a sequence of arguments
/// to a function value, handling currying automatically.
pub fn apply_spine(mut function: Value, arguments: Vec<Value>) -> EvalResult<Value> {
    for arg in arguments {
        function = apply_value(function, arg)?;
    }
    Ok(function)
}

/// Check if a value can be applied (is a function-like value)
#[inline]
pub fn is_applicable(value: &Value) -> bool {
    matches!(value, Value::Lambda(_) | Value::Neutral(_) | Value::Var(_))
}

/// Apply a value with lazy evaluation of the argument
///
/// The argument is only evaluated if the function actually needs it.
/// This enables more efficient evaluation in some cases.
pub fn apply_lazy(function: Value, argument_thunk: impl FnOnce() -> EvalResult<Value>) -> EvalResult<Value> {
    match function {
        Value::Lambda(closure) => {
            // Need to evaluate argument for beta reduction
            let arg_value = argument_thunk()?;
            instantiate_closure(&closure, arg_value)
        },

        Value::Neutral(neutral) => {
            // Need to evaluate argument to extend spine
            let arg_value = argument_thunk()?;
            let applied = neutral.apply(arg_value);
            Ok(Value::Neutral(applied))
        },

        Value::Var(level) => {
            // Need to evaluate argument for neutral term
            let arg_value = argument_thunk()?;
            let neutral = Neutral::new(level).apply(arg_value);
            Ok(Value::Neutral(neutral))
        },

        _ => {
            Err(EvalError::NotAFunction(function))
        }
    }
}

/// Project a field from a record value (if we had records)
///
/// Placeholder for potential future extension with record types.
/// Currently just returns an error.
pub fn project_field(_record: Value, _field: &str) -> EvalResult<Value> {
    Err(EvalError::TypeMismatch("Records not yet implemented".to_string()))
}

/// Eliminate a sum type value (if we had sum types)
///
/// Placeholder for potential future extension with sum types.
/// Currently just returns an error.
pub fn case_split(_scrutinee: Value, _cases: Vec<(String, Closure)>) -> EvalResult<Value> {
    Err(EvalError::TypeMismatch("Sum types not yet implemented".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Level;

    #[test]
    fn test_apply_lambda() {
        // (λx.x) Type₀ → Type₀
        let identity = Closure::empty(Term::var(0));
        let lambda = Value::lambda(identity);
        let arg = Value::universe(Level::TYPE);

        let result = apply_value(lambda, arg.clone()).unwrap();
        assert_eq!(result, arg);
    }

    #[test]
    fn test_apply_neutral() {
        // (#0 Type₀) creates neutral term with spine
        let var = Value::var(0);
        let arg = Value::universe(Level::TYPE);

        let result = apply_value(var, arg.clone()).unwrap();

        match result {
            Value::Neutral(neutral) => {
                assert_eq!(neutral.head, 0);
                assert_eq!(neutral.spine.len(), 1);
                assert_eq!(*neutral.spine[0], arg);
            },
            _ => panic!("Expected neutral value"),
        }
    }

    #[test]
    fn test_apply_non_function() {
        // Type₀ is not a function
        let universe = Value::universe(Level::TYPE);
        let arg = Value::var(0);

        let result = apply_value(universe, arg);
        assert!(matches!(result, Err(EvalError::NotAFunction(_))));
    }

    #[test]
    fn test_instantiate_closure() {
        // Closure with body x should evaluate to the argument
        let closure = Closure::empty(Term::var(0));
        let value = Value::universe(Level::TYPE);

        let result = instantiate_closure(&closure, value.clone()).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_apply_spine() {
        // Apply multiple arguments to a curried function
        // λx.λy.x applied to Type₀ and Type₁
        let inner = Term::lambda(Term::var(1)); // λy.x
        let outer = Term::lambda(inner); // λx.λy.x
        let env = Environment::new();
        let function = super::normalize::evaluate(&outer, &env).unwrap();

        let arg1 = Value::universe(Level::TYPE);
        let arg2 = Value::universe(Level::TYPE.succ());
        let args = vec![arg1.clone(), arg2];

        let result = apply_spine(function, args).unwrap();
        assert_eq!(result, arg1);
    }

    #[test]
    fn test_is_applicable() {
        let lambda = Value::lambda(Closure::empty(Term::var(0)));
        assert!(is_applicable(&lambda));

        let var = Value::var(0);
        assert!(is_applicable(&var));

        let neutral = Value::Neutral(Neutral::new(0));
        assert!(is_applicable(&neutral));

        let universe = Value::universe(Level::TYPE);
        assert!(!is_applicable(&universe));
    }

    #[test]
    fn test_apply_lazy() {
        // Test that lazy application works
        let identity = Value::lambda(Closure::empty(Term::var(0)));
        let thunk_called = std::cell::Cell::new(false);

        let result = apply_lazy(identity, || {
            thunk_called.set(true);
            Ok(Value::universe(Level::TYPE))
        }).unwrap();

        assert!(thunk_called.get());
        assert_eq!(result, Value::universe(Level::TYPE));
    }

    #[test]
    fn test_apply_lazy_not_called() {
        // If we had short-circuiting functions, the thunk might not be called
        // But in our current implementation, it's always called for applicable values
        let universe = Value::universe(Level::TYPE);
        let thunk_called = std::cell::Cell::new(false);

        let result = apply_lazy(universe, || {
            thunk_called.set(true);
            Ok(Value::var(0))
        });

        // Thunk not called because universe is not applicable
        assert!(!thunk_called.get());
        assert!(result.is_err());
    }
}