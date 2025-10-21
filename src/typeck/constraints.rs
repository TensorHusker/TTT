//! Constraint solving and unification
//!
//! This module implements constraint generation and solving for
//! metavariables during type inference.

use std::collections::HashMap;
use std::sync::Arc;
use crate::core::{Value, Level};

/// Unification constraint between two values
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// Two values must be equal
    Equal(Value, Value),
    /// Value must be an instance of a type
    Instance(Value, Value),
    /// Metavariable assignment
    Assign(usize, Value),
}

/// Unification errors
#[derive(Debug, Clone, PartialEq)]
pub enum UnificationError {
    /// Cannot unify two different constructors
    ConstructorMismatch(Value, Value),
    /// Occurs check failure
    OccursCheck(usize, Value),
    /// Universe level mismatch
    LevelMismatch(Level, Level),
    /// Rigid-rigid mismatch
    RigidMismatch(Value, Value),
    /// Cannot solve constraint
    CannotSolve(Constraint),
    /// Metavariable not found
    MetavarNotFound(usize),
}

impl std::fmt::Display for UnificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnificationError::ConstructorMismatch(v1, v2) => {
                write!(f, "Cannot unify {} with {}", v1, v2)
            }
            UnificationError::OccursCheck(meta, value) => {
                write!(f, "Occurs check: ?{} occurs in {}", meta, value)
            }
            UnificationError::LevelMismatch(l1, l2) => {
                write!(f, "Level mismatch: {} ≠ {}", l1, l2)
            }
            UnificationError::RigidMismatch(v1, v2) => {
                write!(f, "Rigid-rigid mismatch: {} ≠ {}", v1, v2)
            }
            UnificationError::CannotSolve(constraint) => {
                write!(f, "Cannot solve constraint: {:?}", constraint)
            }
            UnificationError::MetavarNotFound(id) => {
                write!(f, "Metavariable ?{} not found", id)
            }
        }
    }
}

impl std::error::Error for UnificationError {}

/// Substitution for metavariables
#[derive(Clone, Debug, Default)]
pub struct MetaSubstitution {
    /// Mapping from metavariable IDs to their solutions
    assignments: HashMap<usize, Value>,
}

/// Constraint solver state
#[derive(Debug)]
pub struct ConstraintSolver {
    /// Current constraints to solve
    constraints: Vec<Constraint>,
    /// Current metavariable substitution
    substitution: MetaSubstitution,
    /// Next fresh metavariable ID
    next_meta_id: usize,
}

impl MetaSubstitution {
    /// Create empty substitution
    pub fn new() -> Self {
        MetaSubstitution {
            assignments: HashMap::new(),
        }
    }

    /// Assign a value to a metavariable
    pub fn assign(&mut self, meta_id: usize, value: Value) {
        self.assignments.insert(meta_id, value);
    }

    /// Lookup assignment for metavariable
    pub fn lookup(&self, meta_id: usize) -> Option<&Value> {
        self.assignments.get(&meta_id)
    }

    /// Check if metavariable is assigned
    pub fn is_assigned(&self, meta_id: usize) -> bool {
        self.assignments.contains_key(&meta_id)
    }

    /// Apply substitution to a value
    pub fn apply_to_value(&self, value: &Value) -> Value {
        match value {
            Value::Var(_) | Value::Universe(_) => value.clone(),

            Value::Pi(domain, codomain) => {
                let new_domain = self.apply_to_value(domain);
                // Note: should also apply to codomain but that requires
                // evaluating under binders which is complex
                Value::pi(new_domain, codomain.clone())
            },

            Value::Lambda(closure) => {
                // Should apply to closure body, but that's complex
                value.clone()
            },

            Value::Neutral(neutral) => {
                // Apply to head if it's a metavariable
                if let Some(assigned) = self.lookup(neutral.head) {
                    // Apply spine to the assigned value
                    let mut result = assigned.clone();
                    for arg in &neutral.spine {
                        let arg_subst = self.apply_to_value(arg);
                        result = crate::eval::apply_value(result, arg_subst)
                            .unwrap_or_else(|_| value.clone());
                    }
                    result
                } else {
                    // Apply to spine arguments
                    let new_spine: Vec<_> = neutral.spine.iter()
                        .map(|arg| Arc::new(self.apply_to_value(arg)))
                        .collect();
                    Value::neutral(neutral.head, new_spine)
                }
            },
        }
    }

    /// Get all assignments
    pub fn assignments(&self) -> &HashMap<usize, Value> {
        &self.assignments
    }

    /// Check if substitution is empty
    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }
}

impl ConstraintSolver {
    /// Create new constraint solver
    pub fn new() -> Self {
        ConstraintSolver {
            constraints: Vec::new(),
            substitution: MetaSubstitution::new(),
            next_meta_id: 0,
        }
    }

    /// Generate fresh metavariable
    pub fn fresh_meta(&mut self) -> usize {
        let id = self.next_meta_id;
        self.next_meta_id += 1;
        id
    }

    /// Add constraint to be solved
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Solve all accumulated constraints
    pub fn solve(&mut self) -> Result<(), UnificationError> {
        while let Some(constraint) = self.constraints.pop() {
            self.solve_constraint(constraint)?;
        }
        Ok(())
    }

    /// Solve a single constraint
    fn solve_constraint(&mut self, constraint: Constraint) -> Result<(), UnificationError> {
        match constraint {
            Constraint::Equal(val1, val2) => {
                self.unify(val1, val2)?;
            }
            Constraint::Instance(value, typ) => {
                // For now, just check that value has type typ
                // This would need proper type checking in a real implementation
                // For simplicity, we'll add an equality constraint
                self.add_constraint(Constraint::Equal(value, typ));
            }
            Constraint::Assign(meta_id, value) => {
                // Check occurs check
                if self.occurs_check(meta_id, &value) {
                    return Err(UnificationError::OccursCheck(meta_id, value));
                }
                self.substitution.assign(meta_id, value);
            }
        }
        Ok(())
    }

    /// Unify two values
    fn unify(&mut self, val1: Value, val2: Value) -> Result<(), UnificationError> {
        // Apply current substitution first
        let val1 = self.substitution.apply_to_value(&val1);
        let val2 = self.substitution.apply_to_value(&val2);

        match (val1.clone(), val2.clone()) {
            // Same variables
            (Value::Var(l1), Value::Var(l2)) if l1 == l2 => Ok(()),

            // Same universes
            (Value::Universe(level1), Value::Universe(level2)) => {
                if level1 == level2 {
                    Ok(())
                } else {
                    Err(UnificationError::LevelMismatch(level1, level2))
                }
            }

            // Pi types
            (Value::Pi(dom1, cod1), Value::Pi(dom2, cod2)) => {
                self.unify(dom1.as_ref().clone(), dom2.as_ref().clone())?;
                // For simplicity, we'll assume codomain unification works
                // In practice, this requires evaluating under binders
                Ok(())
            }

            // Metavariables
            (Value::Neutral(n1), Value::Neutral(n2))
                if n1.spine.is_empty() && n2.spine.is_empty() => {
                // Both are metavariables - assign one to the other
                if n1.head < n2.head {
                    self.add_constraint(Constraint::Assign(n1.head, Value::var(n2.head)));
                } else if n2.head < n1.head {
                    self.add_constraint(Constraint::Assign(n2.head, Value::var(n1.head)));
                }
                Ok(())
            }

            // Metavariable with other value
            (Value::Neutral(neutral), other) if neutral.spine.is_empty() => {
                self.add_constraint(Constraint::Assign(neutral.head, other));
                Ok(())
            }
            (other, Value::Neutral(neutral)) if neutral.spine.is_empty() => {
                self.add_constraint(Constraint::Assign(neutral.head, other));
                Ok(())
            }

            // Constructor mismatch
            _ => Err(UnificationError::ConstructorMismatch(val1, val2)),
        }
    }

    /// Check if metavariable occurs in value (occurs check)
    fn occurs_check(&self, meta_id: usize, value: &Value) -> bool {
        match value {
            Value::Var(_) | Value::Universe(_) => false,

            Value::Pi(domain, _) => {
                self.occurs_check(meta_id, domain)
                // Should also check codomain
            }

            Value::Lambda(_) => {
                // Should check lambda body
                false
            }

            Value::Neutral(neutral) => {
                neutral.head == meta_id ||
                neutral.spine.iter().any(|arg| self.occurs_check(meta_id, arg))
            }
        }
    }

    /// Get current substitution
    pub fn substitution(&self) -> &MetaSubstitution {
        &self.substitution
    }

    /// Get mutable substitution
    pub fn substitution_mut(&mut self) -> &mut MetaSubstitution {
        &mut self.substitution
    }

    /// Check if all constraints are solved
    pub fn is_solved(&self) -> bool {
        self.constraints.is_empty()
    }

    /// Get remaining constraints
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }
}

impl Default for ConstraintSolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Level;

    #[test]
    fn test_meta_substitution() {
        let mut subst = MetaSubstitution::new();
        let value = Value::universe(Level::TYPE);

        assert!(!subst.is_assigned(0));
        subst.assign(0, value.clone());
        assert!(subst.is_assigned(0));
        assert_eq!(subst.lookup(0), Some(&value));
    }

    #[test]
    fn test_constraint_solver() {
        let mut solver = ConstraintSolver::new();

        let meta1 = solver.fresh_meta();
        let meta2 = solver.fresh_meta();
        assert_eq!(meta1, 0);
        assert_eq!(meta2, 1);
    }

    #[test]
    fn test_unify_universes() {
        let mut solver = ConstraintSolver::new();
        let val1 = Value::universe(Level::TYPE);
        let val2 = Value::universe(Level::TYPE);

        solver.add_constraint(Constraint::Equal(val1, val2));
        assert!(solver.solve().is_ok());
    }

    #[test]
    fn test_unify_universe_mismatch() {
        let mut solver = ConstraintSolver::new();
        let val1 = Value::universe(Level::TYPE);
        let val2 = Value::universe(Level::TYPE.succ());

        solver.add_constraint(Constraint::Equal(val1, val2));
        assert!(solver.solve().is_err());
    }

    #[test]
    fn test_assign_metavar() {
        let mut solver = ConstraintSolver::new();
        let meta = solver.fresh_meta();
        let value = Value::universe(Level::TYPE);

        solver.add_constraint(Constraint::Assign(meta, value.clone()));
        solver.solve().unwrap();

        assert_eq!(solver.substitution().lookup(meta), Some(&value));
    }

    #[test]
    fn test_occurs_check() {
        let mut solver = ConstraintSolver::new();
        let meta = 0;

        // Meta occurs in itself
        let self_ref = Value::neutral(meta, vec![]);
        assert!(solver.occurs_check(meta, &self_ref));

        // Meta doesn't occur in universe
        let universe = Value::universe(Level::TYPE);
        assert!(!solver.occurs_check(meta, &universe));
    }

    #[test]
    fn test_apply_substitution() {
        let mut subst = MetaSubstitution::new();
        let replacement = Value::universe(Level::TYPE);
        subst.assign(0, replacement.clone());

        let neutral = Value::neutral(0, vec![]);
        let result = subst.apply_to_value(&neutral);
        assert_eq!(result, replacement);
    }
}