//! Normalization by Evaluation (NbE) to Weak Head Normal Form

use crate::{term::Term, Level, Arc};
use alloc::vec::Vec;

/// Values in the semantic domain
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Neutral values (variables applied to arguments)
    Neutral(Neutral),
    /// Lambda closures
    Lambda {
        env: Env,
        body: Arc<Term>,
    },
    /// Pi types
    Pi {
        domain: Arc<Value>,
        closure: Closure,
    },
    /// Sigma types  
    Sigma {
        first: Arc<Value>,
        closure: Closure,
    },
    /// Pairs
    Pair {
        first: Arc<Value>,
        second: Arc<Value>,
    },
    /// Identity types
    Id {
        ty: Arc<Value>,
        left: Arc<Value>,
        right: Arc<Value>,
    },
    /// Reflexivity
    Refl(Arc<Value>),
    /// Universe
    Universe(Level),
}

/// Neutral values (cannot be reduced further)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Neutral {
    /// Variable
    Var(Level),
    /// Application of neutral to value
    App {
        func: Arc<Neutral>,
        arg: Arc<Value>,
    },
    /// First projection of neutral
    Fst(Arc<Neutral>),
    /// Second projection of neutral
    Snd(Arc<Neutral>),
}

/// Closure for representing functions in the semantic domain
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    env: Env,
    body: Arc<Term>,
}

/// Environment mapping levels to values
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Env {
    values: Vec<Arc<Value>>,
}

impl Env {
    /// Create empty environment
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
        }
    }
    
    /// Extend environment
    pub fn extend(&mut self, value: Arc<Value>) -> Level {
        let level = self.values.len() as Level;
        self.values.push(value);
        level
    }
    
    /// Lookup value at level
    pub fn get(&self, level: Level) -> Option<&Arc<Value>> {
        self.values.get(level as usize)
    }
}

impl Closure {
    /// Create new closure
    pub fn new(env: Env, body: Arc<Term>) -> Self {
        Self { env, body }
    }
    
    /// Apply closure to a value
    pub fn apply(&self, arg: Arc<Value>) -> Arc<Value> {
        let mut env = self.env.clone();
        env.extend(arg);
        eval(&env, &self.body)
    }
}

/// Normalizer implementing NbE
pub struct Normalizer {
    next_level: Level,
}

impl Normalizer {
    /// Create new normalizer
    pub fn new() -> Self {
        Self { next_level: 0 }
    }
    
    /// Normalize term to WHNF
    pub fn normalize(&mut self, term: &Arc<Term>) -> Arc<Term> {
        let env = Env::new();
        let value = eval(&env, term);
        self.quote(&value)
    }
    
    /// Check alpha equivalence of two terms (after normalization)
    pub fn alpha_equiv(&mut self, term1: &Arc<Term>, term2: &Arc<Term>) -> bool {
        let norm1 = self.normalize(term1);
        let norm2 = self.normalize(term2);
        norm1 == norm2
    }
    
    /// Quote value back to term
    fn quote(&mut self, value: &Arc<Value>) -> Arc<Term> {
        match value.as_ref() {
            Value::Neutral(neutral) => self.quote_neutral(neutral),
            Value::Lambda { env: _, body } => {
                let var = Arc::new(Value::Neutral(Neutral::Var(self.next_level)));
                self.next_level += 1;
                let closure = Closure::new(Env::new(), body.clone());
                let body_value = closure.apply(var);
                let quoted_body = self.quote(&body_value);
                self.next_level -= 1;
                Term::lambda(None, quoted_body)
            }
            Value::Pi { domain, closure } => {
                let quoted_domain = self.quote(domain);
                let var = Arc::new(Value::Neutral(Neutral::Var(self.next_level)));
                self.next_level += 1;
                let codomain_value = closure.apply(var);
                let quoted_codomain = self.quote(&codomain_value);
                self.next_level -= 1;
                Term::pi(None, quoted_domain, quoted_codomain)
            }
            Value::Sigma { first, closure } => {
                let quoted_first = self.quote(first);
                let var = Arc::new(Value::Neutral(Neutral::Var(self.next_level)));
                self.next_level += 1;
                let second_value = closure.apply(var);
                let quoted_second = self.quote(&second_value);
                self.next_level -= 1;
                Term::sigma(None, quoted_first, quoted_second)
            }
            Value::Pair { first, second } => {
                Term::pair(self.quote(first), self.quote(second))
            }
            Value::Id { ty, left, right } => {
                Term::id(self.quote(ty), self.quote(left), self.quote(right))
            }
            Value::Refl(value) => Term::refl(self.quote(value)),
            Value::Universe(level) => Term::universe(*level),
        }
    }
    
    /// Quote neutral value back to term
    fn quote_neutral(&mut self, neutral: &Neutral) -> Arc<Term> {
        match neutral {
            Neutral::Var(level) => Term::var(*level),
            Neutral::App { func, arg } => {
                Term::app(self.quote_neutral(func), self.quote(&*arg))
            }
            Neutral::Fst(neutral) => Term::fst(self.quote_neutral(neutral)),
            Neutral::Snd(neutral) => Term::snd(self.quote_neutral(neutral)),
        }
    }
}

impl Default for Normalizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate term to value
fn eval(env: &Env, term: &Arc<Term>) -> Arc<Value> {
    match term.as_ref() {
        Term::Var(level) => {
            env.get(*level)
                .cloned()
                .unwrap_or_else(|| Arc::new(Value::Neutral(Neutral::Var(*level))))
        }
        Term::Pi { domain, codomain, .. } => {
            let domain_val = eval(env, domain);
            let closure = Closure::new(env.clone(), codomain.clone());
            Arc::new(Value::Pi {
                domain: domain_val,
                closure,
            })
        }
        Term::Lambda { body, .. } => Arc::new(Value::Lambda {
            env: env.clone(),
            body: body.clone(),
        }),
        Term::App { func, arg } => {
            let func_val = eval(env, func);
            let arg_val = eval(env, arg);
            apply(func_val, arg_val)
        }
        Term::Sigma { first, second, .. } => {
            let first_val = eval(env, first);
            let closure = Closure::new(env.clone(), second.clone());
            Arc::new(Value::Sigma {
                first: first_val,
                closure,
            })
        }
        Term::Pair { first, second } => Arc::new(Value::Pair {
            first: eval(env, first),
            second: eval(env, second),
        }),
        Term::Fst(pair) => {
            let pair_val = eval(env, pair);
            fst(pair_val)
        }
        Term::Snd(pair) => {
            let pair_val = eval(env, pair);
            snd(pair_val)
        }
        Term::Id { ty, left, right } => Arc::new(Value::Id {
            ty: eval(env, ty),
            left: eval(env, left),
            right: eval(env, right),
        }),
        Term::Refl(value) => Arc::new(Value::Refl(eval(env, value))),
        Term::Universe(level) => Arc::new(Value::Universe(*level)),
    }
}

/// Apply function value to argument
fn apply(func: Arc<Value>, arg: Arc<Value>) -> Arc<Value> {
    match func.as_ref() {
        Value::Lambda { env, body } => {
            let mut new_env = env.clone();
            new_env.extend(arg);
            eval(&new_env, body)
        }
        Value::Neutral(neutral) => Arc::new(Value::Neutral(Neutral::App {
            func: Arc::new(neutral.clone()),
            arg,
        })),
        _ => {
            // For ill-typed terms, return as neutral application
            Arc::new(Value::Neutral(Neutral::App {
                func: Arc::new(Neutral::Var(0)), // Dummy neutral
                arg,
            }))
        }
    }
}

/// First projection
fn fst(pair: Arc<Value>) -> Arc<Value> {
    match pair.as_ref() {
        Value::Pair { first, .. } => first.clone(),
        Value::Neutral(neutral) => Arc::new(Value::Neutral(Neutral::Fst(Arc::new(neutral.clone())))),
        _ => {
            // For ill-typed terms, return as neutral
            Arc::new(Value::Neutral(Neutral::Fst(Arc::new(Neutral::Var(0)))))
        }
    }
}

/// Second projection
fn snd(pair: Arc<Value>) -> Arc<Value> {
    match pair.as_ref() {
        Value::Pair { second, .. } => second.clone(),
        Value::Neutral(neutral) => Arc::new(Value::Neutral(Neutral::Snd(Arc::new(neutral.clone())))),
        _ => {
            // For ill-typed terms, return as neutral
            Arc::new(Value::Neutral(Neutral::Snd(Arc::new(Neutral::Var(0)))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_identity() {
        let mut normalizer = Normalizer::new();
        
        // λx. x
        let var0 = Term::var(0);
        let identity = Term::lambda(None, var0.clone());
        
        let normalized = normalizer.normalize(&identity);
        // Should normalize to itself (already in normal form)
        assert_eq!(normalized, identity);
    }
    
    #[test]
    fn test_alpha_equivalence() {
        let mut normalizer = Normalizer::new();
        
        // λx. x and λy. y should be alpha-equivalent
        let id1 = Term::lambda(None, Term::var(0));
        let id2 = Term::lambda(None, Term::var(0));
        
        assert!(normalizer.alpha_equiv(&id1, &id2));
    }
}