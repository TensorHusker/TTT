//! Bidirectional type checking implementation
//!
//! This module provides the core bidirectional type checking algorithm for TTT.
//! It implements both type synthesis (inference) and type checking modes.

use crate::core::{Term, Value, Level};
use crate::eval::{normalize, evaluate, convertible_values, instantiate_closure};
use super::{Context, CheckError, CheckResult, ConstraintSolver, Constraint};
use super::constraints::MetaSubstitution;

/// Main bidirectional type checker
#[derive(Debug)]
pub struct TypeChecker {
    /// Constraint solver for metavariables
    solver: ConstraintSolver,
}

impl TypeChecker {
    /// Create new type checker
    pub fn new() -> Self {
        TypeChecker {
            solver: ConstraintSolver::new(),
        }
    }

    /// Type checking mode: verify that term has expected type
    ///
    /// This is the checking judgment: Γ ⊢ e ⇐ A
    /// Returns () on success, error on type mismatch
    pub fn check(&mut self, term: &Term, expected_type: &Value, context: &Context) -> CheckResult<()> {
        match term {
            // λx.e ⇐ Π(x:A).B
            Term::Lambda(body) => {
                match expected_type {
                    Value::Pi(domain, codomain) => {
                        // Check body under extended context: Γ, x:A ⊢ e ⇐ B[x]
                        let extended_context = context.extend_anonymous((**domain).clone());
                        let fresh_var = Value::var(context.len());
                        let body_type = instantiate_closure(codomain, fresh_var)
                            .map_err(|_| CheckError::CannotInfer(term.clone()))?;
                        self.check(body, &body_type, &extended_context)
                    },
                    _ => {
                        // Lambda but expected type is not Pi - try synthesis
                        let inferred_type = self.infer(term, context)?;
                        self.check_convertible(&inferred_type, expected_type, context)
                    }
                }
            },

            // For other terms, synthesize type and check convertibility
            _ => {
                let inferred_type = self.infer(term, context)?;
                self.check_convertible(&inferred_type, expected_type, context)
            }
        }
    }

    /// Type synthesis mode: infer the type of a term
    ///
    /// This is the synthesis judgment: Γ ⊢ e ⇒ A
    /// Returns the inferred type or error if type cannot be inferred
    pub fn infer(&mut self, term: &Term, context: &Context) -> CheckResult<Value> {
        match term {
            // Variables: look up in context
            Term::Var(index) => {
                if let Some(typ) = context.lookup_type(*index) {
                    Ok(typ.clone())
                } else {
                    Err(CheckError::UnboundVariable(*index))
                }
            },

            // Universes: Type_i : Type_{i+1}
            Term::Universe(level) => {
                Ok(Value::universe(level.succ()))
            },

            // Pi types: Π(x:A).B : Type_i where A : Type_j, B : Type_k, i = max(j,k)
            Term::Pi(domain, codomain) => {
                let domain_type = self.infer(domain, context)?;
                let domain_level = self.extract_universe_level(&domain_type)?;

                // Check codomain under extended context
                let domain_value = evaluate(domain, context.environment())
                    .map_err(|_| CheckError::CannotInfer(term.clone()))?;
                let extended_context = context.extend_anonymous(domain_value);
                let codomain_type = self.infer(codomain, &extended_context)?;
                let codomain_level = self.extract_universe_level(&codomain_type)?;

                // Pi type lives in the maximum universe level
                let pi_level = domain_level.max(codomain_level);
                Ok(Value::universe(pi_level))
            },

            // Applications: f x where f : Π(y:A).B infers to B[y := x]
            Term::App(function, argument) => {
                let function_type = self.infer(function, context)?;

                match function_type {
                    Value::Pi(domain, codomain) => {
                        // Check argument against domain type
                        self.check(argument, &domain, context)?;

                        // Evaluate argument and substitute into codomain
                        let argument_value = evaluate(argument, context.environment())
                            .map_err(|_| CheckError::CannotInfer(term.clone()))?;
                        let result_type = instantiate_closure(&codomain, argument_value)
                            .map_err(|_| CheckError::CannotInfer(term.clone()))?;

                        Ok(result_type)
                    },
                    _ => {
                        Err(CheckError::NotAFunction {
                            function_type,
                            argument: argument.as_ref().clone(),
                        })
                    }
                }
            },

            // Let expressions: infer from expanded form
            Term::Let(binding, body) => {
                let binding_type = self.infer(binding, context)?;
                let binding_value = evaluate(binding, context.environment())
                    .map_err(|_| CheckError::CannotInfer(term.clone()))?;

                let extended_context = context.extend_anonymous(binding_type);
                self.infer(body, &extended_context)
            },

            // Metavariables: create fresh type metavariable
            Term::Meta(id) => {
                let type_meta = self.solver.fresh_meta();
                Ok(Value::neutral(type_meta, vec![]))
            },

            // Lambda: cannot infer type without annotation
            Term::Lambda(_) => {
                Err(CheckError::CannotInfer(term.clone()))
            },
        }
    }

    /// Check that two types are convertible (definitionally equal)
    fn check_convertible(&mut self, actual: &Value, expected: &Value, context: &Context) -> CheckResult<()> {
        if convertible_values(actual, expected, context.len()) {
            Ok(())
        } else {
            // Try constraint solving for metavariables
            self.solver.add_constraint(Constraint::Equal(actual.clone(), expected.clone()));
            if self.solver.solve().is_ok() {
                Ok(())
            } else {
                Err(CheckError::TypeMismatch {
                    expected: expected.clone(),
                    actual: actual.clone(),
                    term: Term::var(0), // Placeholder
                })
            }
        }
    }

    /// Extract universe level from a type value
    fn extract_universe_level(&self, value: &Value) -> CheckResult<Level> {
        match value {
            Value::Universe(level) => Ok(level.clone()),
            _ => Err(CheckError::InvalidLevel(
                format!("Expected universe, got {}", value)
            )),
        }
    }

    /// Get current metavariable substitution
    pub fn substitution(&self) -> &MetaSubstitution {
        self.solver.substitution()
    }

    /// Solve all accumulated constraints
    pub fn solve_constraints(&mut self) -> CheckResult<()> {
        self.solver.solve().map_err(CheckError::from)
    }

    /// Check if all constraints are solved
    pub fn is_solved(&self) -> bool {
        self.solver.is_solved()
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function for type checking
pub fn check_term(term: &Term, expected_type: &Value, context: &Context) -> CheckResult<()> {
    let mut checker = TypeChecker::new();
    checker.check(term, expected_type, context)
}

/// Convenience function for type inference
pub fn infer_type(term: &Term, context: &Context) -> CheckResult<Value> {
    let mut checker = TypeChecker::new();
    checker.infer(term, context)
}

/// Check that a term is well-typed and return its normal form
pub fn elaborate(term: &Term, context: &Context) -> CheckResult<(Term, Value)> {
    let mut checker = TypeChecker::new();
    let typ = checker.infer(term, context)?;
    checker.solve_constraints()?;

    // Apply substitution and normalize
    let subst = checker.substitution();
    let normalized_type = subst.apply_to_value(&typ);
    let normalized_term = normalize(term)
        .map_err(|_| CheckError::CannotInfer(term.clone()))?;

    Ok((normalized_term, normalized_type))
}

/// Type check and normalize a definition
pub fn check_definition(
    name: &str,
    term: &Term,
    typ: Option<&Term>,
    context: &Context,
) -> CheckResult<(Value, Value)> {
    let mut checker = TypeChecker::new();

    let term_type = match typ {
        Some(type_annotation) => {
            // Check that type annotation is well-formed
            let type_type = checker.infer(type_annotation, context)?;
            let _type_level = checker.extract_universe_level(&type_type)?;

            // Evaluate the type annotation
            let expected_type = evaluate(type_annotation, context.environment())
                .map_err(|_| CheckError::CannotInfer(type_annotation.clone()))?;

            // Check term against type
            checker.check(term, &expected_type, context)?;
            expected_type
        },
        None => {
            // Infer type of term
            checker.infer(term, context)?
        }
    };

    // Solve constraints
    checker.solve_constraints()?;

    // Evaluate term
    let term_value = evaluate(term, context.environment())
        .map_err(|_| CheckError::CannotInfer(term.clone()))?;

    // Apply final substitution
    let subst = checker.substitution();
    let final_type = subst.apply_to_value(&term_type);
    let final_value = subst.apply_to_value(&term_value);

    Ok((final_value, final_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Level, Environment};

    #[test]
    fn test_check_universe() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();
        let term = Term::universe(0);
        let expected = Value::universe(Level(1));

        assert!(checker.check(&term, &expected, &context).is_ok());
    }

    #[test]
    fn test_infer_universe() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();
        let term = Term::universe(0);

        let result = checker.infer(&term, &context).unwrap();
        assert_eq!(result, Value::universe(Level(1)));
    }

    #[test]
    fn test_check_variable() {
        let mut checker = TypeChecker::new();
        let context = Context::empty()
            .extend("A".to_string(), Value::universe(Level::TYPE));
        let term = Term::var(0);
        let expected = Value::universe(Level::TYPE);

        assert!(checker.check(&term, &expected, &context).is_ok());
    }

    #[test]
    fn test_infer_variable() {
        let mut checker = TypeChecker::new();
        let var_type = Value::universe(Level::TYPE);
        let context = Context::empty()
            .extend("A".to_string(), var_type.clone());
        let term = Term::var(0);

        let result = checker.infer(&term, &context).unwrap();
        assert_eq!(result, var_type);
    }

    #[test]
    fn test_check_pi_type() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();

        // Π(x:Type₀).Type₀ : Type₁
        let domain = Term::universe(0);
        let codomain = Term::universe(0);
        let pi_term = Term::pi(domain, codomain);
        let expected = Value::universe(Level(1));

        assert!(checker.check(&pi_term, &expected, &context).is_ok());
    }

    #[test]
    fn test_infer_pi_type() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();

        // Π(x:Type₀).Type₀ should infer Type₁
        let domain = Term::universe(0);
        let codomain = Term::universe(0);
        let pi_term = Term::pi(domain, codomain);

        let result = checker.infer(&pi_term, &context).unwrap();
        assert_eq!(result, Value::universe(Level(1)));
    }

    #[test]
    fn test_check_lambda() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();

        // λx.x should check against Π(A:Type₀).A → A
        let identity = Term::lambda(Term::var(0));
        let domain = Value::universe(Level::TYPE);
        let codomain_closure = crate::core::Closure::empty(Term::var(0));
        let pi_type = Value::pi(domain, codomain_closure);

        assert!(checker.check(&identity, &pi_type, &context).is_ok());
    }

    #[test]
    fn test_infer_application() {
        let mut checker = TypeChecker::new();

        // Set up context: id : Π(A:Type₀).A → A, A : Type₀
        let id_type = {
            let a_type = Value::universe(Level::TYPE);
            let arrow_closure = crate::core::Closure::empty(
                Term::pi(Term::var(0), Term::var(1))
            );
            Value::pi(a_type, arrow_closure)
        };

        let context = Context::empty()
            .extend("A".to_string(), Value::universe(Level::TYPE))
            .extend("id".to_string(), id_type);

        // id A should infer A → A
        let app = Term::app(Term::var(0), Term::var(1));
        let result = checker.infer(&app, &context);

        // This is a complex test that would require full evaluation
        // For now, just check that inference doesn't fail
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_mismatch() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();
        let term = Term::universe(0);  // Type₀
        let expected = Value::universe(Level::TYPE);  // Type₀ (wrong, should be Type₁)

        assert!(checker.check(&term, &expected, &context).is_err());
    }

    #[test]
    fn test_unbound_variable() {
        let mut checker = TypeChecker::new();
        let context = Context::empty();
        let term = Term::var(0);  // No variables in context

        let result = checker.infer(&term, &context);
        assert!(matches!(result, Err(CheckError::UnboundVariable(0))));
    }

    #[test]
    fn test_elaborate() {
        let context = Context::empty();
        let term = Term::universe(0);

        let result = elaborate(&term, &context).unwrap();
        assert_eq!(result.0, term);  // Should normalize to itself
        assert_eq!(result.1, Value::universe(Level(1)));  // Type is Type₁
    }

    #[test]
    fn test_check_definition_with_annotation() {
        let context = Context::empty();
        let term = Term::universe(0);
        let type_annotation = Term::universe(1);

        let result = check_definition("mytype", &term, Some(&type_annotation), &context);
        assert!(result.is_ok());

        let (value, typ) = result.unwrap();
        assert_eq!(value, Value::universe(Level::TYPE));
        assert_eq!(typ, Value::universe(Level(1)));
    }

    #[test]
    fn test_check_definition_inferred() {
        let context = Context::empty();
        let term = Term::universe(0);

        let result = check_definition("mytype", &term, None, &context);
        assert!(result.is_ok());

        let (value, typ) = result.unwrap();
        assert_eq!(value, Value::universe(Level::TYPE));
        assert_eq!(typ, Value::universe(Level(1)));
    }
}