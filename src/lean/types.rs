//! Lean type representations for TTT-Lean bridge

use std::fmt;
use serde::{Serialize, Deserialize};

/// Lean term representation
///
/// This mirrors Lean's expression structure but simplified for
/// our translation needs. Uses named variables instead of De Bruijn indices.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LeanTerm {
    /// Variable reference by name
    Var(LeanName),

    /// Universe sort at given level
    Sort(LeanLevel),

    /// Constant from the Lean environment
    Const(LeanName),

    /// Function application
    App(Box<LeanTerm>, Box<LeanTerm>),

    /// Lambda abstraction with explicit type
    Lambda(LeanName, Box<LeanTerm>, Box<LeanTerm>),

    /// Dependent function type (forall)
    Pi(LeanName, Box<LeanTerm>, Box<LeanTerm>),

    /// Let binding
    Let(LeanName, Box<LeanTerm>, Box<LeanTerm>, Box<LeanTerm>),
}

impl LeanTerm {
    /// Create a variable term
    pub fn var(name: impl Into<LeanName>) -> Self {
        LeanTerm::Var(name.into())
    }

    /// Create a sort term
    pub fn sort(level: LeanLevel) -> Self {
        LeanTerm::Sort(level)
    }

    /// Create a constant term
    pub fn const_(name: impl Into<LeanName>) -> Self {
        LeanTerm::Const(name.into())
    }

    /// Create an application term
    pub fn app(func: LeanTerm, arg: LeanTerm) -> Self {
        LeanTerm::App(Box::new(func), Box::new(arg))
    }

    /// Create a lambda term
    pub fn lambda(name: impl Into<LeanName>, ty: LeanTerm, body: LeanTerm) -> Self {
        LeanTerm::Lambda(name.into(), Box::new(ty), Box::new(body))
    }

    /// Create a Pi type
    pub fn pi(name: impl Into<LeanName>, domain: LeanTerm, codomain: LeanTerm) -> Self {
        LeanTerm::Pi(name.into(), Box::new(domain), Box::new(codomain))
    }

    /// Create a let term
    pub fn let_in(name: impl Into<LeanName>, ty: LeanTerm, value: LeanTerm, body: LeanTerm) -> Self {
        LeanTerm::Let(name.into(), Box::new(ty), Box::new(value), Box::new(body))
    }

    /// Check if this is a universe sort
    pub fn is_sort(&self) -> bool {
        matches!(self, LeanTerm::Sort(_))
    }

    /// Check if this is a variable
    pub fn is_var(&self) -> bool {
        matches!(self, LeanTerm::Var(_))
    }

    /// Check if this is a constant
    pub fn is_const(&self) -> bool {
        matches!(self, LeanTerm::Const(_))
    }

    /// Check if this is an application
    pub fn is_app(&self) -> bool {
        matches!(self, LeanTerm::App(_, _))
    }

    /// Check if this is a lambda
    pub fn is_lambda(&self) -> bool {
        matches!(self, LeanTerm::Lambda(_, _, _))
    }

    /// Check if this is a Pi type
    pub fn is_pi(&self) -> bool {
        matches!(self, LeanTerm::Pi(_, _, _))
    }

    /// Collect all free variables in the term
    pub fn free_vars(&self) -> std::collections::HashSet<LeanName> {
        self.free_vars_impl(&std::collections::HashSet::new())
    }

    fn free_vars_impl(&self, bound: &std::collections::HashSet<LeanName>) -> std::collections::HashSet<LeanName> {
        use std::collections::HashSet;

        match self {
            LeanTerm::Var(name) => {
                if bound.contains(name) {
                    HashSet::new()
                } else {
                    let mut vars = HashSet::new();
                    vars.insert(name.clone());
                    vars
                }
            },
            LeanTerm::Sort(_) | LeanTerm::Const(_) => HashSet::new(),
            LeanTerm::App(f, x) => {
                let mut vars = f.free_vars_impl(bound);
                vars.extend(x.free_vars_impl(bound));
                vars
            },
            LeanTerm::Lambda(name, ty, body) | LeanTerm::Pi(name, ty, body) => {
                let mut vars = ty.free_vars_impl(bound);
                let mut new_bound = bound.clone();
                new_bound.insert(name.clone());
                vars.extend(body.free_vars_impl(&new_bound));
                vars
            },
            LeanTerm::Let(name, ty, val, body) => {
                let mut vars = ty.free_vars_impl(bound);
                vars.extend(val.free_vars_impl(bound));
                let mut new_bound = bound.clone();
                new_bound.insert(name.clone());
                vars.extend(body.free_vars_impl(&new_bound));
                vars
            },
        }
    }

    /// Substitute a variable with another term
    pub fn substitute(&self, var: &LeanName, replacement: &LeanTerm) -> LeanTerm {
        self.substitute_impl(var, replacement, &std::collections::HashSet::new())
    }

    fn substitute_impl(&self, var: &LeanName, replacement: &LeanTerm, bound: &std::collections::HashSet<LeanName>) -> LeanTerm {
        use std::collections::HashSet;

        match self {
            LeanTerm::Var(name) => {
                if name == var && !bound.contains(name) {
                    replacement.clone()
                } else {
                    self.clone()
                }
            },
            LeanTerm::Sort(_) | LeanTerm::Const(_) => self.clone(),
            LeanTerm::App(f, x) => {
                LeanTerm::App(
                    Box::new(f.substitute_impl(var, replacement, bound)),
                    Box::new(x.substitute_impl(var, replacement, bound))
                )
            },
            LeanTerm::Lambda(name, ty, body) => {
                let new_ty = ty.substitute_impl(var, replacement, bound);
                if name == var {
                    // Variable is shadowed, don't substitute in body
                    LeanTerm::Lambda(name.clone(), Box::new(new_ty), body.clone())
                } else {
                    let mut new_bound = bound.clone();
                    new_bound.insert(name.clone());
                    let new_body = body.substitute_impl(var, replacement, &new_bound);
                    LeanTerm::Lambda(name.clone(), Box::new(new_ty), Box::new(new_body))
                }
            },
            LeanTerm::Pi(name, dom, cod) => {
                let new_dom = dom.substitute_impl(var, replacement, bound);
                if name == var {
                    LeanTerm::Pi(name.clone(), Box::new(new_dom), cod.clone())
                } else {
                    let mut new_bound = bound.clone();
                    new_bound.insert(name.clone());
                    let new_cod = cod.substitute_impl(var, replacement, &new_bound);
                    LeanTerm::Pi(name.clone(), Box::new(new_dom), Box::new(new_cod))
                }
            },
            LeanTerm::Let(name, ty, val, body) => {
                let new_ty = ty.substitute_impl(var, replacement, bound);
                let new_val = val.substitute_impl(var, replacement, bound);
                if name == var {
                    LeanTerm::Let(name.clone(), Box::new(new_ty), Box::new(new_val), body.clone())
                } else {
                    let mut new_bound = bound.clone();
                    new_bound.insert(name.clone());
                    let new_body = body.substitute_impl(var, replacement, &new_bound);
                    LeanTerm::Let(name.clone(), Box::new(new_ty), Box::new(new_val), Box::new(new_body))
                }
            },
        }
    }
}

/// Lean universe level
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LeanLevel {
    /// Level 0
    Zero,
    /// Successor of a level
    Succ(Box<LeanLevel>),
    /// Maximum of two levels
    Max(Box<LeanLevel>, Box<LeanLevel>),
    /// Impredicative maximum (for Prop)
    IMax(Box<LeanLevel>, Box<LeanLevel>),
    /// Parameter level (for polymorphism)
    Param(LeanName),
}

impl LeanLevel {
    /// Create level 0
    pub fn zero() -> Self {
        LeanLevel::Zero
    }

    /// Create successor level
    pub fn succ(level: LeanLevel) -> Self {
        LeanLevel::Succ(Box::new(level))
    }

    /// Create level from natural number
    pub fn from_nat(n: u32) -> Self {
        let mut level = LeanLevel::Zero;
        for _ in 0..n {
            level = LeanLevel::Succ(Box::new(level));
        }
        level
    }

    /// Create max of two levels
    pub fn max(l1: LeanLevel, l2: LeanLevel) -> Self {
        LeanLevel::Max(Box::new(l1), Box::new(l2))
    }

    /// Create parameter level
    pub fn param(name: impl Into<LeanName>) -> Self {
        LeanLevel::Param(name.into())
    }

    /// Create impredicative maximum level
    pub fn imax(l1: LeanLevel, l2: LeanLevel) -> Self {
        LeanLevel::IMax(Box::new(l1), Box::new(l2))
    }

    /// Try to extract natural number if this is a concrete level
    pub fn to_nat(&self) -> Option<u32> {
        match self {
            LeanLevel::Zero => Some(0),
            LeanLevel::Succ(level) => level.to_nat().map(|n| n + 1),
            _ => None, // Can't extract nat from Max or Param
        }
    }

    /// Check if this is level zero
    pub fn is_zero(&self) -> bool {
        matches!(self, LeanLevel::Zero)
    }
}

/// Lean name representation optimized for performance
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LeanName {
    /// String-based name for readability
    String(String),
    /// Symbol reference for interned strings (optimization)
    Symbol(usize),
    /// Variable index for De Bruijn-style variables
    Idx(usize),
}

impl LeanName {
    /// Create a new Lean name from string
    pub fn new(name: impl Into<String>) -> Self {
        LeanName::String(name.into())
    }

    /// Create a symbol reference for interned strings
    pub fn symbol(id: usize) -> Self {
        LeanName::Symbol(id)
    }

    /// Create a variable index reference
    pub fn idx(index: usize) -> Self {
        LeanName::Idx(index)
    }

    /// Get the string representation (may be expensive for symbols)
    pub fn as_str(&self) -> String {
        match self {
            LeanName::String(s) => s.clone(),
            LeanName::Symbol(id) => format!("symbol_{}", id),
            LeanName::Idx(idx) => format!("var_{}", idx),
        }
    }

    /// Convert to string
    pub fn to_string(&self) -> String {
        self.as_str()
    }

    /// Check if this is an anonymous name
    pub fn is_anonymous(&self) -> bool {
        match self {
            LeanName::String(s) => s == "_",
            _ => false,
        }
    }

    /// Check if this is a symbol reference
    pub fn is_symbol(&self) -> bool {
        matches!(self, LeanName::Symbol(_))
    }

    /// Check if this is a variable index
    pub fn is_idx(&self) -> bool {
        matches!(self, LeanName::Idx(_))
    }
}

impl From<String> for LeanName {
    fn from(name: String) -> Self {
        LeanName::new(name)
    }
}

impl From<&str> for LeanName {
    fn from(name: &str) -> Self {
        LeanName::new(name)
    }
}

impl fmt::Display for LeanName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for LeanLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeanLevel::Zero => write!(f, "0"),
            LeanLevel::Succ(level) => {
                if let Some(n) = self.to_nat() {
                    write!(f, "{}", n)
                } else {
                    write!(f, "succ({})", level)
                }
            },
            LeanLevel::Max(l1, l2) => write!(f, "max({}, {})", l1, l2),
            LeanLevel::IMax(l1, l2) => write!(f, "imax({}, {})", l1, l2),
            LeanLevel::Param(name) => write!(f, "{}", name),
        }
    }
}

impl fmt::Display for LeanTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LeanTerm::Var(name) => write!(f, "{}", name),
            LeanTerm::Sort(level) => write!(f, "Sort {}", level),
            LeanTerm::Const(name) => write!(f, "{}", name),
            LeanTerm::App(func, arg) => write!(f, "({} {})", func, arg),
            LeanTerm::Lambda(name, ty, body) => {
                write!(f, "(λ {} : {}, {})", name, ty, body)
            },
            LeanTerm::Pi(name, domain, codomain) => {
                write!(f, "(Π {} : {}, {})", name, domain, codomain)
            },
            LeanTerm::Let(name, ty, val, body) => {
                write!(f, "(let {} : {} := {} in {})", name, ty, val, body)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_lean_level() {
        let zero = LeanLevel::zero();
        assert!(zero.is_zero());
        assert_eq!(zero.to_nat(), Some(0));

        let one = LeanLevel::succ(zero.clone());
        assert!(!one.is_zero());
        assert_eq!(one.to_nat(), Some(1));

        let five = LeanLevel::from_nat(5);
        assert_eq!(five.to_nat(), Some(5));
    }

    #[test]
    fn test_lean_name() {
        let name = LeanName::new("x");
        assert_eq!(name.as_str(), "x");
        assert!(!name.is_anonymous());

        let anon = LeanName::new("_");
        assert!(anon.is_anonymous());
    }

    #[test]
    fn test_lean_term_construction() {
        let var_x = LeanTerm::var("x");
        assert!(var_x.is_var());

        let type_0 = LeanTerm::sort(LeanLevel::zero());
        assert!(type_0.is_sort());

        let identity = LeanTerm::lambda("x", type_0.clone(), var_x.clone());
        assert!(identity.is_lambda());

        let pi_type = LeanTerm::pi("A", type_0, LeanTerm::pi("_", var_x, LeanTerm::var("A")));
        assert!(pi_type.is_pi());
    }

    #[test]
    fn test_free_variables() {
        let term = LeanTerm::lambda("x", LeanTerm::var("A"), LeanTerm::var("x"));
        let free_vars = term.free_vars();
        assert_eq!(free_vars.len(), 1);
        assert!(free_vars.contains(&LeanName::new("A")));
    }

    #[test]
    fn test_substitution() {
        let term = LeanTerm::app(LeanTerm::var("f"), LeanTerm::var("x"));
        let substituted = term.substitute(&LeanName::new("x"), &LeanTerm::var("y"));

        match substituted {
            LeanTerm::App(f, x) => {
                assert_eq!(*f, LeanTerm::var("f"));
                assert_eq!(*x, LeanTerm::var("y"));
            },
            _ => panic!("Expected application"),
        }
    }
}