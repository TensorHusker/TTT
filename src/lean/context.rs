//! Translation context for TTT-Lean bridge

use std::collections::HashMap;
use crate::core::name::{Name, NameContext};
use crate::lean::types::{LeanName, LeanLevel};
use crate::lean::error::{LeanError, Result};

/// Translation context maintaining mappings and state
#[derive(Debug, Clone)]
pub struct TranslationContext {
    /// Variable name mappings (TTT -> Lean)
    name_map: HashMap<Name, LeanName>,
    /// Reverse name mapping (Lean -> TTT)
    reverse_name_map: HashMap<LeanName, Name>,
    /// Universe level correspondence
    level_map: HashMap<u32, LeanLevel>,
    /// Current binding depth for De Bruijn indices
    binding_depth: usize,
    /// Fresh name counter
    fresh_counter: usize,
    /// Name context for tracking bound variables
    name_context: NameContext,
}

impl TranslationContext {
    pub fn new() -> Self {
        Self {
            name_map: HashMap::new(),
            reverse_name_map: HashMap::new(),
            level_map: Self::init_universe_levels(),
            binding_depth: 0,
            fresh_counter: 0,
            name_context: NameContext::new(),
        }
    }

    fn init_universe_levels() -> HashMap<u32, LeanLevel> {
        (0..20).map(|i| (i, LeanLevel::from_nat(i))).collect()
    }

    /// Generate a fresh name
    pub fn fresh_name(&mut self, hint: &str) -> LeanName {
        self.fresh_counter += 1;
        LeanName::new(format!("{}_{}", hint, self.fresh_counter))
    }

    /// Enter a new binding scope
    pub fn enter_binder(&mut self, ttt_name: Name) -> LeanName {
        self.binding_depth += 1;

        // Generate a fresh Lean name based on the TTT name
        let lean_name = if ttt_name.is_anonymous() {
            self.fresh_name("_anon")
        } else {
            // Try to use the original name, but make it fresh if needed
            let hint = ttt_name.as_str();
            if self.reverse_name_map.contains_key(&LeanName::new(hint)) {
                self.fresh_name(hint)
            } else {
                LeanName::new(hint)
            }
        };

        // Update mappings
        self.name_map.insert(ttt_name.clone(), lean_name.clone());
        self.reverse_name_map.insert(lean_name.clone(), ttt_name.clone());

        // Update name context
        self.name_context = self.name_context.bind(ttt_name);

        lean_name
    }

    /// Exit a binding scope
    pub fn exit_binder(&mut self) {
        if self.binding_depth > 0 {
            self.binding_depth -= 1;

            // Remove the most recent binding from context
            if let Some(name) = self.name_context.names().last() {
                let name = name.clone();
                if let Some(lean_name) = self.name_map.remove(&name) {
                    self.reverse_name_map.remove(&lean_name);
                }
            }

            // Update name context (create new one without the last binding)
            let names = self.name_context.names();
            if !names.is_empty() {
                let mut new_context = NameContext::new();
                for name in &names[..names.len() - 1] {
                    new_context = new_context.bind(name.clone());
                }
                self.name_context = new_context;
            } else {
                self.name_context = NameContext::new();
            }
        }
    }

    /// Look up a TTT name to get the corresponding Lean name
    pub fn lookup_name(&self, ttt_name: &Name) -> Option<&LeanName> {
        self.name_map.get(ttt_name)
    }

    /// Look up a Lean name to get the corresponding TTT name
    pub fn reverse_lookup_name(&self, lean_name: &LeanName) -> Option<&Name> {
        self.reverse_name_map.get(lean_name)
    }

    /// Convert a De Bruijn index to a Lean name
    pub fn debruijn_to_lean(&self, index: usize) -> Result<LeanName> {
        if let Some(ttt_name) = self.name_context.name_at(index) {
            if let Some(lean_name) = self.lookup_name(ttt_name) {
                Ok(lean_name.clone())
            } else {
                Err(LeanError::name_resolution(format!(
                    "No Lean name mapping for TTT name: {}", ttt_name
                )))
            }
        } else {
            Err(LeanError::name_resolution(format!(
                "De Bruijn index {} out of bounds (depth: {})",
                index, self.name_context.depth()
            )))
        }
    }

    /// Convert a Lean name to a De Bruijn index
    pub fn lean_to_debruijn(&self, lean_name: &LeanName) -> Result<usize> {
        if let Some(ttt_name) = self.reverse_lookup_name(lean_name) {
            if let Some(index) = self.name_context.lookup(ttt_name) {
                Ok(index)
            } else {
                Err(LeanError::name_resolution(format!(
                    "TTT name {} not found in context", ttt_name
                )))
            }
        } else {
            Err(LeanError::name_resolution(format!(
                "No TTT name mapping for Lean name: {}", lean_name
            )))
        }
    }

    /// Get the Lean level for a universe level
    pub fn get_universe_level(&self, level: u32) -> Result<LeanLevel> {
        self.level_map.get(&level)
            .cloned()
            .ok_or_else(|| LeanError::UniverseLevel(level))
    }

    /// Get the current binding depth
    pub fn binding_depth(&self) -> usize {
        self.binding_depth
    }

    /// Get the name context
    pub fn name_context(&self) -> &NameContext {
        &self.name_context
    }

    /// Create a scoped context for entering a binder
    pub fn with_binder<T>(&mut self, ttt_name: Name, f: impl FnOnce(&mut Self, LeanName) -> T) -> T {
        let lean_name = self.enter_binder(ttt_name);
        let result = f(self, lean_name);
        self.exit_binder();
        result
    }

    /// Register a global name mapping (for constants, etc.)
    pub fn register_global(&mut self, ttt_name: Name, lean_name: LeanName) {
        self.name_map.insert(ttt_name.clone(), lean_name.clone());
        self.reverse_name_map.insert(lean_name, ttt_name);
    }

    /// Check if a name is bound in the current context
    pub fn is_bound(&self, ttt_name: &Name) -> bool {
        self.name_context.lookup(ttt_name).is_some()
    }

    /// Get all bound names
    pub fn bound_names(&self) -> &[Name] {
        self.name_context.names()
    }

    /// Reset the context (for context pool optimization)
    pub fn reset(&mut self) {
        self.name_map.clear();
        self.reverse_name_map.clear();
        self.binding_depth = 0;
        self.fresh_counter = 0;
        self.name_context = NameContext::new();
    }

    /// Look up a variable by index (for optimized translation)
    pub fn lookup_variable(&self, index: usize) -> Result<LeanName> {
        self.debruijn_to_lean(index)
    }
}

impl Default for TranslationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binder_management() {
        let mut ctx = TranslationContext::new();
        assert_eq!(ctx.binding_depth(), 0);

        let ttt_name = Name::user("x");
        let lean_name = ctx.enter_binder(ttt_name.clone());

        assert_eq!(ctx.binding_depth(), 1);
        assert!(ctx.is_bound(&ttt_name));
        assert_eq!(ctx.lookup_name(&ttt_name), Some(&lean_name));
        assert_eq!(ctx.reverse_lookup_name(&lean_name), Some(&ttt_name));

        ctx.exit_binder();
        assert_eq!(ctx.binding_depth(), 0);
        assert!(!ctx.is_bound(&ttt_name));
    }

    #[test]
    fn test_debruijn_conversion() {
        let mut ctx = TranslationContext::new();

        let x = Name::user("x");
        let y = Name::user("y");

        let lean_x = ctx.enter_binder(x.clone());
        let lean_y = ctx.enter_binder(y.clone());

        // y is at index 0 (most recent), x is at index 1
        assert_eq!(ctx.debruijn_to_lean(0).unwrap(), lean_y);
        assert_eq!(ctx.debruijn_to_lean(1).unwrap(), lean_x);

        assert_eq!(ctx.lean_to_debruijn(&lean_y).unwrap(), 0);
        assert_eq!(ctx.lean_to_debruijn(&lean_x).unwrap(), 1);
    }

    #[test]
    fn test_universe_levels() {
        let ctx = TranslationContext::new();

        assert_eq!(ctx.get_universe_level(0).unwrap(), LeanLevel::from_nat(0));
        assert_eq!(ctx.get_universe_level(5).unwrap(), LeanLevel::from_nat(5));

        // Level too high should fail
        assert!(ctx.get_universe_level(100).is_err());
    }

    #[test]
    fn test_scoped_binder() {
        let mut ctx = TranslationContext::new();

        let result = ctx.with_binder(Name::user("x"), |ctx, lean_name| {
            assert_eq!(ctx.binding_depth(), 1);
            assert_eq!(lean_name.as_str(), "x");
            "test_result"
        });

        assert_eq!(result, "test_result");
        assert_eq!(ctx.binding_depth(), 0);
    }

    #[test]
    fn test_fresh_name_generation() {
        let mut ctx = TranslationContext::new();

        let name1 = ctx.fresh_name("test");
        let name2 = ctx.fresh_name("test");

        assert_ne!(name1, name2);
        assert!(name1.as_str().starts_with("test_"));
        assert!(name2.as_str().starts_with("test_"));
    }
}