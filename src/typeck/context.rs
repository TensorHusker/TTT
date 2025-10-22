//! Type checking context and environment management
//!
//! This module provides data structures for maintaining typing contexts
//! during bidirectional type checking.

use crate::core::{Value, Environment, DeBruijnIndex, DeBruijnLevel};

/// Entry in the typing context
///
/// Contains both the name (for debugging) and the type of a bound variable.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextEntry {
    /// Variable name (for pretty printing and debugging)
    pub name: String,
    /// Type of the variable
    pub typ: Value,
}

/// Typing context for bidirectional type checking
///
/// Maintains both the types of variables and their values for evaluation.
/// Uses De Bruijn indices for variables where index 0 refers to the
/// most recently bound variable.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    /// Entries for each bound variable (outermost first)
    entries: Vec<ContextEntry>,
    /// Evaluation environment corresponding to the types
    environment: Environment,
}

/// Local context for temporary extensions
///
/// Lightweight wrapper that allows extending context without
/// copying the entire structure. Useful for checking under binders.
#[derive(Clone, Debug)]
pub struct LocalContext<'a> {
    /// Base context
    base: &'a Context,
    /// Additional entries
    extensions: Vec<ContextEntry>,
    /// Values for the extensions
    extension_values: Vec<Value>,
}

impl ContextEntry {
    /// Create a new context entry
    pub fn new(name: String, typ: Value) -> Self {
        ContextEntry { name, typ }
    }

    /// Create an anonymous entry
    pub fn anonymous(typ: Value) -> Self {
        ContextEntry::new("_".to_string(), typ)
    }
}

impl Context {
    /// Create an empty context
    pub fn empty() -> Self {
        Context {
            entries: Vec::new(),
            environment: Environment::new(),
        }
    }

    /// Get the length of the context
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if context is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Extend context with a new binding
    ///
    /// The new binding becomes accessible at De Bruijn index 0.
    pub fn extend(&self, name: String, typ: Value) -> Self {
        let entry = ContextEntry::new(name, typ.clone());
        let mut new_entries = self.entries.clone();
        new_entries.push(entry);

        let new_environment = self.environment.extend(typ);

        Context {
            entries: new_entries,
            environment: new_environment,
        }
    }

    /// Extend context with an anonymous binding
    pub fn extend_anonymous(&self, typ: Value) -> Self {
        self.extend("_".to_string(), typ)
    }

    /// Lookup variable type by De Bruijn index
    ///
    /// Index 0 refers to the most recently bound variable.
    pub fn lookup_type(&self, index: DeBruijnIndex) -> Option<&Value> {
        if index < self.entries.len() {
            let entry_index = self.entries.len() - 1 - index;
            Some(&self.entries[entry_index].typ)
        } else {
            None
        }
    }

    /// Lookup variable name by De Bruijn index
    pub fn lookup_name(&self, index: DeBruijnIndex) -> Option<&String> {
        if index < self.entries.len() {
            let entry_index = self.entries.len() - 1 - index;
            Some(&self.entries[entry_index].name)
        } else {
            None
        }
    }

    /// Lookup variable value by De Bruijn index
    ///
    /// Returns the value stored in the evaluation environment.
    pub fn lookup_value(&self, index: DeBruijnIndex) -> Option<&Value> {
        self.environment.lookup(index)
    }

    /// Get the evaluation environment
    #[inline]
    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    /// Convert De Bruijn index to level in this context
    pub fn index_to_level(&self, index: DeBruijnIndex) -> Option<DeBruijnLevel> {
        if index < self.len() {
            Some(self.len() - 1 - index)
        } else {
            None
        }
    }

    /// Convert De Bruijn level to index in this context
    pub fn level_to_index(&self, level: DeBruijnLevel) -> Option<DeBruijnIndex> {
        if level < self.len() {
            Some(self.len() - 1 - level)
        } else {
            None
        }
    }

    /// Get all entries
    pub fn entries(&self) -> &[ContextEntry] {
        &self.entries
    }

    /// Create a fresh variable at the current context level
    ///
    /// Returns a variable value that represents a fresh variable
    /// at the outermost level of this context.
    pub fn fresh_var(&self) -> Value {
        Value::var(self.len())
    }

    /// Extend context with multiple bindings
    pub fn extend_many(&self, bindings: Vec<(String, Value)>) -> Self {
        bindings.into_iter().fold(self.clone(), |ctx, (name, typ)| {
            ctx.extend(name, typ)
        })
    }

    /// Check if context contains a binding with given name
    pub fn contains_name(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }

    /// Find the De Bruijn index of a variable by name
    ///
    /// Returns the index of the most recent binding with the given name.
    pub fn find_by_name(&self, name: &str) -> Option<DeBruijnIndex> {
        for (i, entry) in self.entries.iter().rev().enumerate() {
            if entry.name == name {
                return Some(i);
            }
        }
        None
    }
}

impl<'a> LocalContext<'a> {
    /// Create a new local context extending the base
    pub fn new(base: &'a Context) -> Self {
        LocalContext {
            base,
            extensions: Vec::new(),
            extension_values: Vec::new(),
        }
    }

    /// Extend local context with a new binding
    pub fn extend(&mut self, name: String, typ: Value) {
        self.extensions.push(ContextEntry::new(name, typ.clone()));
        self.extension_values.push(typ);
    }

    /// Extend with anonymous binding
    pub fn extend_anonymous(&mut self, typ: Value) {
        self.extend("_".to_string(), typ);
    }

    /// Get total length including extensions
    pub fn len(&self) -> usize {
        self.base.len() + self.extensions.len()
    }

    /// Lookup type by De Bruijn index
    pub fn lookup_type(&self, index: DeBruijnIndex) -> Option<&Value> {
        if index < self.extensions.len() {
            // Look in extensions first (most recent)
            let ext_index = self.extensions.len() - 1 - index;
            Some(&self.extensions[ext_index].typ)
        } else {
            // Look in base context
            let base_index = index - self.extensions.len();
            self.base.lookup_type(base_index)
        }
    }

    /// Lookup name by De Bruijn index
    pub fn lookup_name(&self, index: DeBruijnIndex) -> Option<&String> {
        if index < self.extensions.len() {
            let ext_index = self.extensions.len() - 1 - index;
            Some(&self.extensions[ext_index].name)
        } else {
            let base_index = index - self.extensions.len();
            self.base.lookup_name(base_index)
        }
    }

    /// Convert to full context (expensive - copies everything)
    pub fn to_context(&self) -> Context {
        self.extensions.iter().fold(self.base.clone(), |ctx, entry| {
            ctx.extend(entry.name.clone(), entry.typ.clone())
        })
    }

    /// Get the base context
    pub fn base(&self) -> &Context {
        self.base
    }

    /// Get extensions
    pub fn extensions(&self) -> &[ContextEntry] {
        &self.extensions
    }
}

impl std::fmt::Display for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", entry.name, entry.typ)?;
        }
        write!(f, "]")
    }
}

impl std::fmt::Display for ContextEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.typ)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Level;

    #[test]
    fn test_empty_context() {
        let ctx = Context::empty();
        assert!(ctx.is_empty());
        assert_eq!(ctx.len(), 0);
        assert_eq!(ctx.lookup_type(0), None);
    }

    #[test]
    fn test_extend_context() {
        let ctx = Context::empty();
        let typ = Value::universe(Level::TYPE);
        let ctx1 = ctx.extend("x".to_string(), typ.clone());

        assert_eq!(ctx1.len(), 1);
        assert_eq!(ctx1.lookup_type(0), Some(&typ));
        assert_eq!(ctx1.lookup_name(0), Some(&"x".to_string()));
    }

    #[test]
    fn test_multiple_bindings() {
        let ctx = Context::empty();
        let typ1 = Value::universe(Level::TYPE);
        let typ2 = Value::var(0);

        let ctx1 = ctx.extend("A".to_string(), typ1.clone());
        let ctx2 = ctx1.extend("x".to_string(), typ2.clone());

        assert_eq!(ctx2.len(), 2);
        assert_eq!(ctx2.lookup_type(0), Some(&typ2)); // Most recent
        assert_eq!(ctx2.lookup_type(1), Some(&typ1)); // Previous
        assert_eq!(ctx2.lookup_name(0), Some(&"x".to_string()));
        assert_eq!(ctx2.lookup_name(1), Some(&"A".to_string()));
    }

    #[test]
    fn test_index_level_conversion() {
        let ctx = Context::empty()
            .extend("A".to_string(), Value::universe(Level::TYPE))
            .extend("B".to_string(), Value::universe(Level::TYPE))
            .extend("x".to_string(), Value::var(1));

        assert_eq!(ctx.index_to_level(0), Some(2)); // x -> level 2
        assert_eq!(ctx.index_to_level(1), Some(1)); // B -> level 1
        assert_eq!(ctx.index_to_level(2), Some(0)); // A -> level 0

        assert_eq!(ctx.level_to_index(0), Some(2)); // level 0 -> A
        assert_eq!(ctx.level_to_index(1), Some(1)); // level 1 -> B
        assert_eq!(ctx.level_to_index(2), Some(0)); // level 2 -> x
    }

    #[test]
    fn test_find_by_name() {
        let ctx = Context::empty()
            .extend("A".to_string(), Value::universe(Level::TYPE))
            .extend("x".to_string(), Value::var(0))
            .extend("A".to_string(), Value::var(1)); // Shadow previous A

        assert_eq!(ctx.find_by_name("A"), Some(0)); // Most recent A
        assert_eq!(ctx.find_by_name("x"), Some(1));
        assert_eq!(ctx.find_by_name("y"), None);
    }

    #[test]
    fn test_local_context() {
        let base = Context::empty()
            .extend("A".to_string(), Value::universe(Level::TYPE));

        let mut local = LocalContext::new(&base);
        local.extend("x".to_string(), Value::var(0));

        assert_eq!(local.len(), 2);
        assert_eq!(local.lookup_name(0), Some(&"x".to_string()));
        assert_eq!(local.lookup_name(1), Some(&"A".to_string()));
    }

    #[test]
    fn test_fresh_var() {
        let ctx = Context::empty()
            .extend("A".to_string(), Value::universe(Level::TYPE));

        let fresh = ctx.fresh_var();
        assert_eq!(fresh, Value::var(1));
    }

    #[test]
    fn test_extend_many() {
        let ctx = Context::empty();
        let bindings = vec![
            ("A".to_string(), Value::universe(Level::TYPE)),
            ("B".to_string(), Value::universe(Level::TYPE)),
        ];

        let extended = ctx.extend_many(bindings);
        assert_eq!(extended.len(), 2);
        assert_eq!(extended.lookup_name(0), Some(&"B".to_string()));
        assert_eq!(extended.lookup_name(1), Some(&"A".to_string()));
    }
}