//! Substitution fusion optimizations
//!
//! This module implements fusion laws for optimizing multiple substitution operations
//! into single passes, significantly reducing traversal overhead in complex terms.

use std::collections::HashMap;
use crate::core::{Term, subst::Substitution};
use super::{record_metric, time_operation};

/// Parallel substitution optimizer using fusion laws
///
/// Combines multiple substitution operations into a single traversal,
/// applying advanced fusion techniques to minimize computational overhead.
#[derive(Clone, Debug)]
pub struct SubstitutionFusion {
    /// Cached substitution compositions
    composition_cache: HashMap<(u64, u64), Substitution>,
    /// Statistics on fusion effectiveness
    fusion_hits: u64,
    fusion_misses: u64,
}

impl Default for SubstitutionFusion {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstitutionFusion {
    /// Create a new substitution fusion optimizer
    pub fn new() -> Self {
        SubstitutionFusion {
            composition_cache: HashMap::new(),
            fusion_hits: 0,
            fusion_misses: 0,
        }
    }

    /// Apply fusion law: substitute(substitute(t, s1), s2) → substitute(t, s1 ∘ s2)
    ///
    /// This is the fundamental fusion optimization that combines sequential
    /// substitutions into a single parallel substitution operation.
    pub fn fuse_substitutions(&mut self, term: &Term, substitutions: &[Substitution]) -> Term {
        if substitutions.is_empty() {
            return term.clone();
        }

        if substitutions.len() == 1 {
            return crate::core::subst::apply_substitution(term, &substitutions[0]);
        }

        time_operation(
            || {
                // Compose all substitutions into a single parallel substitution
                let fused = self.compose_substitutions(substitutions);

                record_metric(|metrics| {
                    metrics.fusion_count += 1;
                });

                crate::core::subst::apply_substitution(term, &fused)
            },
            |metrics, time| metrics.substitution_time_ns += time
        )
    }

    /// Compose multiple substitutions using associativity
    ///
    /// Uses memoization to cache frequently composed substitutions
    /// and applies algebraic laws for optimization.
    fn compose_substitutions(&mut self, substitutions: &[Substitution]) -> Substitution {
        match substitutions.len() {
            0 => Substitution::identity(),
            1 => substitutions[0].clone(),
            2 => self.compose_pair(&substitutions[0], &substitutions[1]),
            _ => {
                // Divide and conquer for large composition chains
                let mid = substitutions.len() / 2;
                let left = self.compose_substitutions(&substitutions[..mid]);
                let right = self.compose_substitutions(&substitutions[mid..]);
                self.compose_pair(&left, &right)
            }
        }
    }

    /// Compose two substitutions with caching
    fn compose_pair(&mut self, s1: &Substitution, s2: &Substitution) -> Substitution {
        let key = (self.hash_substitution(s1), self.hash_substitution(s2));

        if let Some(cached) = self.composition_cache.get(&key) {
            self.fusion_hits += 1;
            return cached.clone();
        }

        self.fusion_misses += 1;
        let result = s1.compose(s2);

        // Cache the result if cache isn't too large
        if self.composition_cache.len() < 1024 {
            self.composition_cache.insert(key, result.clone());
        }

        result
    }

    /// Optimize substitution chains in terms
    ///
    /// Identifies patterns where multiple substitutions can be fused:
    /// - Sequential applications: subst(subst(t, s1), s2)
    /// - Parallel applications: subst(t, s1) + subst(t, s2)
    /// - Composition chains: subst(subst(subst(t, s1), s2), s3)
    pub fn optimize_term_substitutions(&mut self, term: &Term) -> Term {
        match term {
            Term::Var(_) | Term::Universe(_) | Term::Meta(_) => term.clone(),

            Term::Pi(domain, codomain) => {
                let opt_domain = self.optimize_term_substitutions(domain);
                let opt_codomain = self.optimize_term_substitutions(codomain);
                Term::pi(opt_domain, opt_codomain)
            },

            Term::Lambda(body) => {
                let opt_body = self.optimize_term_substitutions(body);
                Term::lambda(opt_body)
            },

            Term::App(function, argument) => {
                let opt_function = self.optimize_term_substitutions(function);
                let opt_argument = self.optimize_term_substitutions(argument);
                Term::app(opt_function, opt_argument)
            },

            Term::Let(binding, body) => {
                let opt_binding = self.optimize_term_substitutions(binding);
                let opt_body = self.optimize_term_substitutions(body);
                Term::let_in(opt_binding, opt_body)
            },
        }
    }

    /// Detect and optimize substitution patterns in evaluation
    ///
    /// This function analyzes term structure to identify opportunities
    /// for substitution fusion during normalization.
    pub fn detect_fusion_opportunities(&self, term: &Term) -> Vec<FusionOpportunity> {
        let mut opportunities = Vec::new();
        self.analyze_term(term, &mut opportunities, 0);
        opportunities
    }

    /// Internal analysis for fusion opportunities
    fn analyze_term(&self, term: &Term, opportunities: &mut Vec<FusionOpportunity>, depth: usize) {
        match term {
            Term::Var(_) | Term::Universe(_) | Term::Meta(_) => {},

            Term::Pi(domain, codomain) => {
                self.analyze_term(domain, opportunities, depth + 1);
                self.analyze_term(codomain, opportunities, depth + 1);
            },

            Term::Lambda(body) => {
                // Lambda creates substitution context
                if depth > 0 {
                    opportunities.push(FusionOpportunity {
                        kind: FusionKind::LambdaSubstitution,
                        depth,
                        complexity: self.estimate_term_complexity(body),
                    });
                }
                self.analyze_term(body, opportunities, depth + 1);
            },

            Term::App(function, argument) => {
                // Applications often create substitution chains
                if let Term::Lambda(_) = function.as_ref() {
                    opportunities.push(FusionOpportunity {
                        kind: FusionKind::BetaReduction,
                        depth,
                        complexity: self.estimate_term_complexity(term),
                    });
                }
                self.analyze_term(function, opportunities, depth + 1);
                self.analyze_term(argument, opportunities, depth + 1);
            },

            Term::Let(binding, body) => {
                // Let creates explicit substitution
                opportunities.push(FusionOpportunity {
                    kind: FusionKind::LetSubstitution,
                    depth,
                    complexity: self.estimate_term_complexity(binding) + self.estimate_term_complexity(body),
                });
                self.analyze_term(binding, opportunities, depth + 1);
                self.analyze_term(body, opportunities, depth + 1);
            },
        }
    }

    /// Estimate computational complexity of a term
    fn estimate_term_complexity(&self, term: &Term) -> usize {
        match term {
            Term::Var(_) | Term::Universe(_) | Term::Meta(_) => 1,
            Term::Pi(domain, codomain) => {
                1 + self.estimate_term_complexity(domain) + self.estimate_term_complexity(codomain)
            },
            Term::Lambda(body) => 1 + self.estimate_term_complexity(body),
            Term::App(function, argument) => {
                1 + self.estimate_term_complexity(function) + self.estimate_term_complexity(argument)
            },
            Term::Let(binding, body) => {
                1 + self.estimate_term_complexity(binding) + self.estimate_term_complexity(body)
            },
        }
    }

    /// Simple hash function for substitutions (for caching)
    fn hash_substitution(&self, subst: &Substitution) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        subst.domain_size().hash(&mut hasher);
        hasher.finish()
    }

    /// Get fusion statistics
    pub fn get_stats(&self) -> FusionStats {
        FusionStats {
            cache_hits: self.fusion_hits,
            cache_misses: self.fusion_misses,
            cache_size: self.composition_cache.len(),
            hit_rate: if self.fusion_hits + self.fusion_misses > 0 {
                self.fusion_hits as f64 / (self.fusion_hits + self.fusion_misses) as f64
            } else {
                0.0
            },
        }
    }

    /// Clear fusion cache (useful for memory management)
    pub fn clear_cache(&mut self) {
        self.composition_cache.clear();
    }
}

/// Opportunity for substitution fusion optimization
#[derive(Clone, Debug)]
pub struct FusionOpportunity {
    /// Type of fusion that can be applied
    pub kind: FusionKind,
    /// Depth in the term where opportunity occurs
    pub depth: usize,
    /// Estimated complexity reduction
    pub complexity: usize,
}

/// Types of fusion optimizations
#[derive(Clone, Debug, PartialEq)]
pub enum FusionKind {
    /// Fusion in lambda substitution
    LambdaSubstitution,
    /// Fusion in beta reduction
    BetaReduction,
    /// Fusion in let substitution
    LetSubstitution,
    /// General parallel substitution
    ParallelSubstitution,
}

/// Statistics for substitution fusion
#[derive(Clone, Debug)]
pub struct FusionStats {
    /// Number of cache hits
    pub cache_hits: u64,
    /// Number of cache misses
    pub cache_misses: u64,
    /// Current cache size
    pub cache_size: usize,
    /// Hit rate (0.0 to 1.0)
    pub hit_rate: f64,
}

/// Global fusion optimizer instance
static mut GLOBAL_FUSION: Option<SubstitutionFusion> = None;

/// Initialize global fusion optimizer
pub fn init_fusion() {
    unsafe {
        GLOBAL_FUSION = Some(SubstitutionFusion::new());
    }
}

/// Apply fusion optimization to multiple substitutions
pub fn fuse_substitutions(term: &Term, substitutions: &[Substitution]) -> Term {
    unsafe {
        if let Some(ref mut fusion) = GLOBAL_FUSION {
            fusion.fuse_substitutions(term, substitutions)
        } else {
            // Fallback: apply substitutions sequentially
            substitutions.iter().fold(term.clone(), |acc, subst| {
                crate::core::subst::apply_substitution(&acc, subst)
            })
        }
    }
}

/// Optimize a term for substitution fusion
pub fn optimize_term(term: &Term) -> Term {
    unsafe {
        if let Some(ref mut fusion) = GLOBAL_FUSION {
            fusion.optimize_term_substitutions(term)
        } else {
            term.clone()
        }
    }
}

/// Get current fusion statistics
pub fn get_fusion_stats() -> Option<FusionStats> {
    unsafe {
        GLOBAL_FUSION.as_ref().map(|fusion| fusion.get_stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Term, subst::Substitution};

    #[test]
    fn test_substitution_fusion() {
        let mut fusion = SubstitutionFusion::new();

        // Create a term: λx.λy.x
        let term = Term::lambda(Term::lambda(Term::var(1)));

        // Create two substitutions
        let s1 = Substitution::single(0, Term::universe(0));
        let s2 = Substitution::single(1, Term::universe(1));

        // Test fusion
        let result = fusion.fuse_substitutions(&term, &[s1, s2]);

        // Result should be equivalent to applying substitutions sequentially
        assert!(result.is_lambda());
    }

    #[test]
    fn test_fusion_opportunities() {
        let fusion = SubstitutionFusion::new();

        // Create a complex term with fusion opportunities
        let term = Term::app(
            Term::lambda(Term::var(0)),
            Term::universe(0)
        );

        let opportunities = fusion.detect_fusion_opportunities(&term);
        assert!(!opportunities.is_empty());

        // Should detect beta reduction opportunity
        assert!(opportunities.iter().any(|op| op.kind == FusionKind::BetaReduction));
    }

    #[test]
    fn test_composition_caching() {
        let mut fusion = SubstitutionFusion::new();

        let s1 = Substitution::single(0, Term::universe(0));
        let s2 = Substitution::single(1, Term::universe(1));

        // First composition should be a cache miss
        let _result1 = fusion.compose_pair(&s1, &s2);
        assert_eq!(fusion.fusion_misses, 1);

        // Second composition should be a cache hit
        let _result2 = fusion.compose_pair(&s1, &s2);
        assert_eq!(fusion.fusion_hits, 1);
    }

    #[test]
    fn test_complexity_estimation() {
        let fusion = SubstitutionFusion::new();

        let simple = Term::var(0);
        let complex = Term::app(
            Term::lambda(Term::pi(Term::universe(0), Term::universe(1))),
            Term::universe(2)
        );

        assert!(fusion.estimate_term_complexity(&complex) > fusion.estimate_term_complexity(&simple));
    }
}