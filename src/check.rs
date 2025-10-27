//! Bidirectional type checking

use crate::{term::Term, types::{Type, TypeEnv}, Level, Arena, CheckResult, Arc};
use core::fmt;

/// Type checking errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// Variable not found in context
    UnboundVariable(Level),
    /// Type mismatch
    TypeMismatch {
        expected: Arc<Type>,
        found: Arc<Type>,
    },
    /// Cannot infer type (need annotation)
    CannotInfer(Arc<Term>),
    /// Invalid universe level
    InvalidUniverse(Level, Level),
    /// Application to non-function
    NotAFunction(Arc<Type>),
    /// Projection from non-pair
    NotAPair(Arc<Type>),
    /// Invalid equality proof
    InvalidEquality {
        left: Arc<Term>,
        right: Arc<Term>,
        ty: Arc<Type>,
    },
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnboundVariable(level) => write!(f, "Unbound variable: #{}", level),
            TypeError::TypeMismatch { expected, found } => {
                write!(f, "Type mismatch: expected {}, found {}", expected, found)
            }
            TypeError::CannotInfer(term) => write!(f, "Cannot infer type for: {}", term),
            TypeError::InvalidUniverse(level, max) => {
                write!(f, "Invalid universe level {} (max: {})", level, max)
            }
            TypeError::NotAFunction(ty) => write!(f, "Cannot apply to non-function type: {}", ty),
            TypeError::NotAPair(ty) => write!(f, "Cannot project from non-pair type: {}", ty),
            TypeError::InvalidEquality { left, right, ty } => {
                write!(f, "Invalid equality: {} ≡[{}] {}", left, ty, right)
            }
        }
    }
}

/// Bidirectional type checker
pub struct TypeChecker {
    arena: Arena,
    max_universe: Level,
}

impl TypeChecker {
    /// Create new type checker
    pub fn new(max_universe: Level) -> Self {
        Self {
            arena: Arena::new(),
            max_universe,
        }
    }
    
    /// Infer the type of a term
    pub fn infer(&mut self, env: &TypeEnv, term: &Arc<Term>) -> CheckResult<Type> {
        match term.as_ref() {
            Term::Var(level) => {
                env.get(*level)
                    .cloned()
                    .ok_or_else(|| TypeError::UnboundVariable(*level))
            }
            
            Term::Pi { domain, codomain, .. } => {
                let domain_ty = self.infer(env, domain)?;
                let mut env_ext = env.clone();
                let _ = env_ext.extend(domain.clone());
                let codomain_ty = self.infer(&env_ext, codomain)?;
                
                // Both domain and codomain must be types
                let domain_level = self.get_universe_level(&domain_ty)?;
                let codomain_level = self.get_universe_level(&codomain_ty)?;
                let result_level = domain_level.max(codomain_level);
                
                if result_level > self.max_universe {
                    return Err(TypeError::InvalidUniverse(result_level, self.max_universe));
                }
                
                Ok(Term::universe(result_level))
            }
            
            Term::Lambda { .. } => {
                Err(TypeError::CannotInfer(term.clone()))
            }
            
            Term::App { func, arg } => {
                let func_ty = self.infer(env, func)?;
                match func_ty.as_ref() {
                    Term::Pi { domain, codomain, .. } => {
                        self.check(env, arg, domain)?;
                        Ok(codomain.subst(0, arg))
                    }
                    _ => Err(TypeError::NotAFunction(func_ty)),
                }
            }
            
            Term::Sigma { first, second, .. } => {
                let first_ty = self.infer(env, first)?;
                let mut env_ext = env.clone();
                let _ = env_ext.extend(first.clone());
                let second_ty = self.infer(&env_ext, second)?;
                
                let first_level = self.get_universe_level(&first_ty)?;
                let second_level = self.get_universe_level(&second_ty)?;
                let result_level = first_level.max(second_level);
                
                if result_level > self.max_universe {
                    return Err(TypeError::InvalidUniverse(result_level, self.max_universe));
                }
                
                Ok(Term::universe(result_level))
            }
            
            Term::Pair { .. } => {
                Err(TypeError::CannotInfer(term.clone()))
            }
            
            Term::Fst(pair) => {
                let pair_ty = self.infer(env, pair)?;
                match pair_ty.as_ref() {
                    Term::Sigma { first, .. } => Ok(first.clone()),
                    _ => Err(TypeError::NotAPair(pair_ty)),
                }
            }
            
            Term::Snd(pair) => {
                let pair_ty = self.infer(env, pair)?;
                match pair_ty.as_ref() {
                    Term::Sigma { second, .. } => {
                        let fst_val = Term::fst(pair.clone());
                        Ok(second.subst(0, &fst_val))
                    }
                    _ => Err(TypeError::NotAPair(pair_ty)),
                }
            }
            
            Term::Id { ty, .. } => {
                let ty_ty = self.infer(env, ty)?;
                let level = self.get_universe_level(&ty_ty)?;
                
                if level > self.max_universe {
                    return Err(TypeError::InvalidUniverse(level, self.max_universe));
                }
                
                Ok(Term::universe(level))
            }
            
            Term::Refl(value) => {
                let ty = self.infer(env, value)?;
                Ok(Term::id(ty, value.clone(), value.clone()))
            }
            
            Term::Universe(level) => {
                if *level >= self.max_universe {
                    Err(TypeError::InvalidUniverse(*level, self.max_universe))
                } else {
                    Ok(Term::universe(level + 1))
                }
            }
        }
    }
    
    /// Check that a term has the expected type
    pub fn check(&mut self, env: &TypeEnv, term: &Arc<Term>, expected: &Arc<Type>) -> CheckResult<Type> {
        match (term.as_ref(), expected.as_ref()) {
            (Term::Lambda { body, .. }, Term::Pi { domain, codomain, .. }) => {
                let mut env_ext = env.clone();
                let _ = env_ext.extend(domain.clone());
                self.check(&env_ext, body, codomain)?;
                Ok(expected.clone())
            }
            
            (Term::Pair { first, second }, Term::Sigma { first: exp_first, second: exp_second, .. }) => {
                self.check(env, first, exp_first)?;
                // For non-dependent pairs, we don't need substitution
                self.check(env, second, exp_second)?;
                Ok(expected.clone())
            }
            
            _ => {
                let inferred = self.infer(env, term)?;
                if self.type_equal(&inferred, expected) {
                    Ok(expected.clone())
                } else {
                    Err(TypeError::TypeMismatch {
                        expected: expected.clone(),
                        found: inferred,
                    })
                }
            }
        }
    }
    
    /// Check if two types are equal (structural equality for now)
    fn type_equal(&self, ty1: &Arc<Type>, ty2: &Arc<Type>) -> bool {
        ty1 == ty2
    }
    
    /// Extract universe level from a type
    fn get_universe_level(&self, ty: &Arc<Type>) -> Result<Level, TypeError> {
        match ty.as_ref() {
            Term::Universe(level) => Ok(*level),
            _ => Err(TypeError::CannotInfer(ty.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_infer_universe() {
        let mut checker = TypeChecker::new(10);
        let env = TypeEnv::new();
        
        let type0 = Term::universe(0);
        let result = checker.infer(&env, &type0).unwrap();
        assert_eq!(*result, Term::Universe(1));
    }
    
    #[test]
    fn test_check_lambda() {
        let mut checker = TypeChecker::new(10);
        let env = TypeEnv::new();
        
        // Test a simpler case: λx. x where x : Type0, so λx. x : Type0 -> Type0
        let type0 = Term::universe(0);
        let var0 = Term::var(0);  // This will be bound by the lambda
        let id_body = Term::lambda(None, var0);
        let id_type = Term::pi(None, type0.clone(), type0);  // Type0 -> Type0
        
        let result = checker.check(&env, &id_body, &id_type);
        assert!(result.is_ok());
    }
}