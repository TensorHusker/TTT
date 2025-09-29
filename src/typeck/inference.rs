//! Type inference support and metavariable generation
//!
//! This module provides utilities for type inference, including
//! metavariable generation and inference context management.

use std::cell::RefCell;
use std::rc::Rc;
use crate::core::{Term, Value};
use super::{Context, CheckResult, ConstraintSolver};

/// Global metavariable generator
///
/// Thread-local generator for creating fresh metavariables during type inference.
/// Uses RefCell for interior mutability in a single-threaded context.
thread_local! {
    static META_GENERATOR: RefCell<MetavarGenerator> = RefCell::new(MetavarGenerator::new());
}

/// Metavariable generator
#[derive(Debug)]
pub struct MetavarGenerator {
    next_id: usize,
}

impl MetavarGenerator {
    /// Create new metavariable generator
    pub fn new() -> Self {
        MetavarGenerator { next_id: 0 }
    }

    /// Generate fresh metavariable ID
    pub fn fresh(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Reset generator (for testing)
    pub fn reset(&mut self) {
        self.next_id = 0;
    }

    /// Get current ID (for debugging)
    pub fn current_id(&self) -> usize {
        self.next_id
    }
}

impl Default for MetavarGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a fresh metavariable term
pub fn fresh_meta() -> Term {
    META_GENERATOR.with(|gen| {
        let id = gen.borrow_mut().fresh();
        Term::meta(id)
    })
}

/// Generate a fresh type metavariable
pub fn fresh_type_meta() -> Value {
    META_GENERATOR.with(|gen| {
        let id = gen.borrow_mut().fresh();
        Value::neutral(id, vec![])
    })
}

/// Reset the global metavariable generator (for testing)
pub fn reset_meta_generator() {
    META_GENERATOR.with(|gen| gen.borrow_mut().reset());
}

/// Inference context for tracking metavariables and their constraints
#[derive(Debug, Clone)]
pub struct InferenceContext {
    /// Base typing context
    base_context: Context,
    /// Metavariables and their types
    metavar_types: Vec<Value>,
    /// Constraint solver
    solver: Rc<RefCell<ConstraintSolver>>,
}

impl InferenceContext {
    /// Create new inference context
    pub fn new(base_context: Context) -> Self {
        InferenceContext {
            base_context,
            metavar_types: Vec::new(),
            solver: Rc::new(RefCell::new(ConstraintSolver::new())),
        }
    }

    /// Extend with a regular binding
    pub fn extend(&self, name: String, typ: Value) -> Self {
        InferenceContext {
            base_context: self.base_context.extend(name, typ),
            metavar_types: self.metavar_types.clone(),
            solver: self.solver.clone(),
        }
    }

    /// Add a metavariable with its type
    pub fn add_metavar(&mut self, typ: Value) -> usize {
        let id = self.metavar_types.len();
        self.metavar_types.push(typ);
        id
    }

    /// Lookup metavariable type
    pub fn metavar_type(&self, id: usize) -> Option<&Value> {
        self.metavar_types.get(id)
    }

    /// Get base context
    pub fn context(&self) -> &Context {
        &self.base_context
    }

    /// Get solver
    pub fn solver(&self) -> Rc<RefCell<ConstraintSolver>> {
        self.solver.clone()
    }

    /// Convert to regular context (losing metavariable information)
    pub fn to_context(&self) -> Context {
        self.base_context.clone()
    }
}

/// Type-directed inference for implicit arguments
#[derive(Debug)]
pub struct ImplicitInference {
    /// Current inference context
    context: InferenceContext,
}

impl ImplicitInference {
    /// Create new implicit inference engine
    pub fn new(context: Context) -> Self {
        ImplicitInference {
            context: InferenceContext::new(context),
        }
    }

    /// Infer implicit arguments for a function application
    ///
    /// Given a function of type Π(implicit args).Π(explicit args).result,
    /// infer the implicit arguments and return the residual type.
    pub fn infer_implicit_args(
        &mut self,
        function_type: &Value,
        explicit_args: &[Term],
    ) -> CheckResult<(Vec<Term>, Value)> {
        let current_type = function_type.clone();
        let implicit_args = Vec::new();

        // For now, we don't have implicit argument syntax,
        // so we just return empty implicit args
        // In a full implementation, this would:
        // 1. Check if function type has implicit Pi types
        // 2. Generate metavariables for implicit arguments
        // 3. Apply explicit arguments to get residual type

        Ok((implicit_args, current_type))
    }

    /// Try to infer the type of an expression with holes
    pub fn infer_with_holes(&mut self, term: &Term) -> CheckResult<(Term, Value)> {
        // This would implement bidirectional inference with hole inference
        // For now, just use the basic type checker

        let binding = self.context.solver();
        let solver = binding.borrow_mut();
        let typ = crate::typeck::TypeChecker::new().infer(term, self.context.context())?;

        Ok((term.clone(), typ))
    }

    /// Get current context
    pub fn context(&self) -> &InferenceContext {
        &self.context
    }

    /// Extend context
    pub fn extend(&mut self, name: String, typ: Value) {
        self.context = self.context.extend(name, typ);
    }
}

/// Higher-rank polymorphism support
#[derive(Debug)]
pub struct PolymorphicInference {
    /// Skolem variables for universal quantification
    skolems: Vec<(String, Value)>,
}

impl PolymorphicInference {
    /// Create new polymorphic inference engine
    pub fn new() -> Self {
        PolymorphicInference {
            skolems: Vec::new(),
        }
    }

    /// Introduce a skolem variable for universal quantification
    pub fn introduce_skolem(&mut self, name: String, typ: Value) -> Value {
        let level = self.skolems.len();
        self.skolems.push((name, typ.clone()));
        Value::var(level)
    }

    /// Check if a type contains skolem variables (escaping scope)
    pub fn escapes_scope(&self, typ: &Value) -> bool {
        self.contains_skolem(typ, self.skolems.len())
    }

    /// Check if value contains skolem variables above given level
    fn contains_skolem(&self, value: &Value, max_level: usize) -> bool {
        match value {
            Value::Var(level) => *level < max_level,
            Value::Universe(_) => false,
            Value::Pi(domain, _) => {
                self.contains_skolem(domain, max_level)
                // Should also check codomain
            },
            Value::Lambda(_) => {
                // Should check lambda body
                false
            },
            Value::Neutral(neutral) => {
                neutral.head < max_level ||
                neutral.spine.iter().any(|arg| self.contains_skolem(arg, max_level))
            },
        }
    }

    /// Get skolem variables
    pub fn skolems(&self) -> &[(String, Value)] {
        &self.skolems
    }

    /// Clear all skolem variables
    pub fn clear(&mut self) {
        self.skolems.clear();
    }
}

impl Default for PolymorphicInference {
    fn default() -> Self {
        Self::new()
    }
}

/// Constraint generation for type inference
pub struct ConstraintGenerator {
    constraints: Vec<super::Constraint>,
}

impl ConstraintGenerator {
    /// Create new constraint generator
    pub fn new() -> Self {
        ConstraintGenerator {
            constraints: Vec::new(),
        }
    }

    /// Add equality constraint
    pub fn equal(&mut self, val1: Value, val2: Value) {
        self.constraints.push(super::Constraint::Equal(val1, val2));
    }

    /// Add instance constraint
    pub fn instance(&mut self, value: Value, typ: Value) {
        self.constraints.push(super::Constraint::Instance(value, typ));
    }

    /// Add assignment constraint
    pub fn assign(&mut self, meta_id: usize, value: Value) {
        self.constraints.push(super::Constraint::Assign(meta_id, value));
    }

    /// Get all constraints
    pub fn constraints(&self) -> &[super::Constraint] {
        &self.constraints
    }

    /// Take all constraints
    pub fn take_constraints(&mut self) -> Vec<super::Constraint> {
        std::mem::take(&mut self.constraints)
    }

    /// Clear all constraints
    pub fn clear(&mut self) {
        self.constraints.clear();
    }
}

impl Default for ConstraintGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Level;

    #[test]
    fn test_metavar_generator() {
        let mut gen = MetavarGenerator::new();
        assert_eq!(gen.fresh(), 0);
        assert_eq!(gen.fresh(), 1);
        assert_eq!(gen.fresh(), 2);

        gen.reset();
        assert_eq!(gen.fresh(), 0);
    }

    #[test]
    fn test_fresh_meta() {
        reset_meta_generator();
        let meta1 = fresh_meta();
        let meta2 = fresh_meta();

        assert_eq!(meta1, Term::meta(0));
        assert_eq!(meta2, Term::meta(1));
    }

    #[test]
    fn test_fresh_type_meta() {
        reset_meta_generator();
        let type_meta1 = fresh_type_meta();
        let type_meta2 = fresh_type_meta();

        assert_eq!(type_meta1, Value::neutral(0, vec![]));
        assert_eq!(type_meta2, Value::neutral(1, vec![]));
    }

    #[test]
    fn test_inference_context() {
        let base = Context::empty();
        let mut inf_ctx = InferenceContext::new(base);

        let typ = Value::universe(Level::TYPE);
        let meta_id = inf_ctx.add_metavar(typ.clone());

        assert_eq!(meta_id, 0);
        assert_eq!(inf_ctx.metavar_type(meta_id), Some(&typ));
        assert_eq!(inf_ctx.metavar_type(999), None);
    }

    #[test]
    fn test_inference_context_extend() {
        let base = Context::empty();
        let inf_ctx = InferenceContext::new(base);

        let typ = Value::universe(Level::TYPE);
        let extended = inf_ctx.extend("A".to_string(), typ.clone());

        assert_eq!(extended.context().len(), 1);
        assert_eq!(extended.context().lookup_type(0), Some(&typ));
    }

    #[test]
    fn test_implicit_inference() {
        let context = Context::empty();
        let mut impl_inf = ImplicitInference::new(context);

        // Test basic functionality
        let function_type = Value::universe(Level::TYPE);
        let explicit_args = vec![];

        let result = impl_inf.infer_implicit_args(&function_type, &explicit_args);
        assert!(result.is_ok());

        let (implicit_args, residual_type) = result.unwrap();
        assert!(implicit_args.is_empty());
        assert_eq!(residual_type, function_type);
    }

    #[test]
    fn test_polymorphic_inference() {
        let mut poly_inf = PolymorphicInference::new();

        let typ = Value::universe(Level::TYPE);
        let skolem = poly_inf.introduce_skolem("A".to_string(), typ.clone());

        assert_eq!(skolem, Value::var(0));
        assert_eq!(poly_inf.skolems().len(), 1);
        assert_eq!(poly_inf.skolems()[0], ("A".to_string(), typ));

        // Test scope escaping
        let typ_with_skolem = Value::var(0);
        assert!(poly_inf.escapes_scope(&typ_with_skolem));

        let typ_without_skolem = Value::universe(Level::TYPE);
        assert!(!poly_inf.escapes_scope(&typ_without_skolem));
    }

    #[test]
    fn test_constraint_generator() {
        let mut gen = ConstraintGenerator::new();

        let val1 = Value::universe(Level::TYPE);
        let val2 = Value::universe(Level::TYPE);

        gen.equal(val1.clone(), val2.clone());
        gen.instance(val1.clone(), val2.clone());
        gen.assign(0, val1.clone());

        assert_eq!(gen.constraints().len(), 3);

        let constraints = gen.take_constraints();
        assert_eq!(constraints.len(), 3);
        assert_eq!(gen.constraints().len(), 0);
    }
}