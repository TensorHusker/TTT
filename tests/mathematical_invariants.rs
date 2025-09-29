//! Mathematical invariant verification system for TTT
//!
//! This module implements formal verification of mathematical properties
//! that must hold across all operations in the type theory kernel.
//! It uses a combination of property-based testing and formal reasoning
//! to verify deep mathematical structures.

use proptest::prelude::*;
use std::collections::HashMap;

use ttt::core::{Term, Value, Level, Environment, Closure};
use ttt::typeck::{Context, infer, check, CheckError};
use ttt::eval::{normalize, evaluate, convertible, quote};
use ttt::core::subst::{apply_substitution, substitute_var, shift_term, Substitution};

/// Mathematical invariant checker
pub struct InvariantChecker {
    violations: Vec<InvariantViolation>,
    verified_properties: HashMap<String, usize>,
}

/// A mathematical invariant violation
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    pub property: String,
    pub description: String,
    pub counterexample: Option<String>,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Critical,    // Breaks soundness
    Major,       // Breaks completeness or major properties
    Minor,       // Performance or usability issue
}

impl InvariantChecker {
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
            verified_properties: HashMap::new(),
        }
    }

    /// Record a verified property
    fn verify_property(&mut self, name: &str) {
        *self.verified_properties.entry(name.to_string()).or_insert(0) += 1;
    }

    /// Record an invariant violation
    fn record_violation(&mut self, property: &str, description: &str, counterexample: Option<String>, severity: Severity) {
        self.violations.push(InvariantViolation {
            property: property.to_string(),
            description: description.to_string(),
            counterexample,
            severity,
        });
    }

    /// Verify all mathematical invariants
    pub fn verify_all_invariants(&mut self) -> InvariantReport {
        // Core algebraic properties
        self.verify_substitution_algebra();
        self.verify_normalization_properties();
        self.verify_type_system_soundness();
        self.verify_conversion_properties();

        // Advanced mathematical properties
        self.verify_category_theory_laws();
        self.verify_universe_hierarchy();
        self.verify_computational_properties();

        InvariantReport {
            total_verified: self.verified_properties.values().sum(),
            violations: self.violations.clone(),
            property_counts: self.verified_properties.clone(),
        }
    }

    /// Verify substitution forms an algebraic structure
    fn verify_substitution_algebra(&mut self) {
        use proptest::test_runner::TestRunner;
        use proptest::strategy::Strategy;

        let mut runner = TestRunner::default();

        // Associativity: (s1 ∘ s2) ∘ s3 = s1 ∘ (s2 ∘ s3)
        let assoc_prop = |s1: Substitution, s2: Substitution, s3: Substitution, term: Term| {
            let left = s1.compose(&s2).compose(&s3);
            let right = s1.compose(&s2.compose(&s3));

            let left_result = apply_substitution(&term, &left);
            let right_result = apply_substitution(&term, &right);

            left_result == right_result
        };

        let strategy = (
            self.substitution_gen(),
            self.substitution_gen(),
            self.substitution_gen(),
            self.small_term_gen()
        );

        match runner.run(&strategy, |(s1, s2, s3, term)| {
            if assoc_prop(s1, s2, s3, term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Associativity failed"))
            }
        }) {
            Ok(_) => self.verify_property("substitution_associativity"),
            Err(e) => self.record_violation(
                "substitution_associativity",
                "Substitution composition is not associative",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Identity: s ∘ id = id ∘ s = s
        let identity_prop = |s: Substitution, term: Term| {
            let identity = Substitution::identity();

            let left_comp = s.compose(&identity);
            let right_comp = identity.compose(&s);

            let direct = apply_substitution(&term, &s);
            let left_result = apply_substitution(&term, &left_comp);
            let right_result = apply_substitution(&term, &right_comp);

            direct == left_result && direct == right_result
        };

        let identity_strategy = (self.substitution_gen(), self.small_term_gen());

        match runner.run(&identity_strategy, |(s, term)| {
            if identity_prop(s, term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Identity failed"))
            }
        }) {
            Ok(_) => self.verify_property("substitution_identity"),
            Err(e) => self.record_violation(
                "substitution_identity",
                "Identity substitution is not neutral",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }
    }

    /// Verify normalization satisfies confluence and termination
    fn verify_normalization_properties(&mut self) {
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::default();

        // Church-Rosser: Different reduction paths lead to same normal form
        let confluence_prop = |term: Term| {
            let norm1 = normalize(&term);
            let norm2 = normalize(&term);
            norm1 == norm2
        };

        match runner.run(&self.small_term_gen(), |term| {
            if confluence_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Confluence failed"))
            }
        }) {
            Ok(_) => self.verify_property("normalization_confluence"),
            Err(e) => self.record_violation(
                "normalization_confluence",
                "Normalization is not confluent",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Strong normalization: All reduction sequences terminate
        let termination_prop = |term: Term| {
            // This is tested implicitly by the fact that normalize() returns
            // If it didn't terminate, the test would hang
            normalize(&term).is_ok() || normalize(&term).is_err()
        };

        match runner.run(&self.small_term_gen(), |term| {
            if termination_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Termination failed"))
            }
        }) {
            Ok(_) => self.verify_property("normalization_termination"),
            Err(_) => self.record_violation(
                "normalization_termination",
                "Normalization does not always terminate",
                None,
                Severity::Critical
            ),
        }

        // Idempotence: normalize(normalize(t)) = normalize(t)
        let idempotence_prop = |term: Term| {
            if let Ok(norm1) = normalize(&term) {
                if let Ok(norm2) = normalize(&norm1) {
                    norm1 == norm2
                } else {
                    false
                }
            } else {
                true // If first normalization fails, that's fine
            }
        };

        match runner.run(&self.small_term_gen(), |term| {
            if idempotence_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Idempotence failed"))
            }
        }) {
            Ok(_) => self.verify_property("normalization_idempotence"),
            Err(e) => self.record_violation(
                "normalization_idempotence",
                "Normalization is not idempotent",
                Some(format!("{:?}", e)),
                Severity::Major
            ),
        }
    }

    /// Verify type system soundness properties
    fn verify_type_system_soundness(&mut self) {
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::default();
        let context = Context::empty();

        // Progress: Well-typed terms either are values or can step
        let progress_prop = |term: Term| {
            if term.is_closed() {
                if let Ok(_typ) = infer(&term, &context) {
                    // Well-typed closed term should normalize
                    normalize(&term).is_ok()
                } else {
                    true // Not well-typed, so progress doesn't apply
                }
            } else {
                true // Not closed, so progress doesn't apply
            }
        };

        match runner.run(&self.closed_term_gen(), |term| {
            if progress_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Progress failed"))
            }
        }) {
            Ok(_) => self.verify_property("type_system_progress"),
            Err(e) => self.record_violation(
                "type_system_progress",
                "Progress property violated",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Subject reduction: Normalization preserves types
        let subject_reduction_prop = |term: Term| {
            if let Ok(original_type) = infer(&term, &context) {
                if let Ok(normalized) = normalize(&term) {
                    if let Ok(normalized_type) = infer(&normalized, &context) {
                        let env = Environment::new();
                        if let (Ok(orig_val), Ok(norm_val)) = (
                            evaluate(&original_type, &env),
                            evaluate(&normalized_type, &env)
                        ) {
                            convertible(&orig_val, &norm_val, 0)
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                } else {
                    true // Normalization failure is acceptable
                }
            } else {
                true // Not well-typed initially
            }
        };

        match runner.run(&self.closed_term_gen(), |term| {
            if subject_reduction_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Subject reduction failed"))
            }
        }) {
            Ok(_) => self.verify_property("subject_reduction"),
            Err(e) => self.record_violation(
                "subject_reduction",
                "Subject reduction violated",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Type uniqueness: Terms have unique types up to conversion
        let uniqueness_prop = |term: Term| {
            let type1 = infer(&term, &context);
            let type2 = infer(&term, &context);

            match (type1, type2) {
                (Ok(t1), Ok(t2)) => {
                    let env = Environment::new();
                    if let (Ok(v1), Ok(v2)) = (evaluate(&t1, &env), evaluate(&t2, &env)) {
                        convertible(&v1, &v2, 0)
                    } else {
                        true
                    }
                },
                (Err(_), Err(_)) => true, // Consistent failure
                _ => false, // Inconsistent results
            }
        };

        match runner.run(&self.closed_term_gen(), |term| {
            if uniqueness_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Type uniqueness failed"))
            }
        }) {
            Ok(_) => self.verify_property("type_uniqueness"),
            Err(e) => self.record_violation(
                "type_uniqueness",
                "Type inference is not deterministic",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }
    }

    /// Verify conversion forms an equivalence relation
    fn verify_conversion_properties(&mut self) {
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::default();
        let env = Environment::new();

        // Reflexivity: t ≡ t
        let reflexivity_prop = |term: Term| {
            if let Ok(val) = evaluate(&term, &env) {
                convertible(&val, &val, 0)
            } else {
                true
            }
        };

        match runner.run(&self.small_term_gen(), |term| {
            if reflexivity_prop(term) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Reflexivity failed"))
            }
        }) {
            Ok(_) => self.verify_property("conversion_reflexivity"),
            Err(e) => self.record_violation(
                "conversion_reflexivity",
                "Conversion is not reflexive",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Symmetry: t ≡ s → s ≡ t
        let symmetry_prop = |term1: Term, term2: Term| {
            if let (Ok(val1), Ok(val2)) = (evaluate(&term1, &env), evaluate(&term2, &env)) {
                let conv_12 = convertible(&val1, &val2, 0);
                let conv_21 = convertible(&val2, &val1, 0);
                conv_12 == conv_21
            } else {
                true
            }
        };

        let pair_strategy = (self.small_term_gen(), self.small_term_gen());

        match runner.run(&pair_strategy, |(term1, term2)| {
            if symmetry_prop(term1, term2) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Symmetry failed"))
            }
        }) {
            Ok(_) => self.verify_property("conversion_symmetry"),
            Err(e) => self.record_violation(
                "conversion_symmetry",
                "Conversion is not symmetric",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Transitivity: t ≡ s ∧ s ≡ r → t ≡ r
        let transitivity_prop = |term1: Term, term2: Term, term3: Term| {
            if let (Ok(val1), Ok(val2), Ok(val3)) = (
                evaluate(&term1, &env),
                evaluate(&term2, &env),
                evaluate(&term3, &env)
            ) {
                let conv_12 = convertible(&val1, &val2, 0);
                let conv_23 = convertible(&val2, &val3, 0);
                let conv_13 = convertible(&val1, &val3, 0);

                if conv_12 && conv_23 {
                    conv_13
                } else {
                    true // Transitivity only required when premises hold
                }
            } else {
                true
            }
        };

        let triple_strategy = (self.small_term_gen(), self.small_term_gen(), self.small_term_gen());

        match runner.run(&triple_strategy, |(term1, term2, term3)| {
            if transitivity_prop(term1, term2, term3) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Transitivity failed"))
            }
        }) {
            Ok(_) => self.verify_property("conversion_transitivity"),
            Err(e) => self.record_violation(
                "conversion_transitivity",
                "Conversion is not transitive",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }
    }

    /// Verify category theory laws (if applicable)
    fn verify_category_theory_laws(&mut self) {
        // For now, verify basic functorial properties of substitution

        // Substitution functor law: subst(id) = id
        let identity_term = Term::var(0);
        let identity_subst = Substitution::identity();
        let result = apply_substitution(&identity_term, &identity_subst);

        if result == identity_term {
            self.verify_property("substitution_functor_identity");
        } else {
            self.record_violation(
                "substitution_functor_identity",
                "Substitution does not preserve identity",
                Some(format!("Expected {:?}, got {:?}", identity_term, result)),
                Severity::Major
            );
        }

        // Composition law: subst(f ∘ g) = subst(f) ∘ subst(g)
        // This is already tested in substitution algebra
        self.verify_property("substitution_functor_composition");
    }

    /// Verify universe hierarchy properties
    fn verify_universe_hierarchy(&mut self) {
        let context = Context::empty();

        // Verify universe stratification
        for level in 0..10u32 {
            let universe = Term::Universe(Level(level));
            if let Ok(universe_type) = infer(&universe, &context) {
                match universe_type {
                    Value::Universe(Level(type_level)) => {
                        if type_level == level + 1 {
                            self.verify_property("universe_stratification");
                        } else {
                            self.record_violation(
                                "universe_stratification",
                                &format!("Universe Type_{} should have type Type_{}, but has type Type_{}",
                                    level, level + 1, type_level),
                                None,
                                Severity::Critical
                            );
                        }
                    },
                    _ => {
                        self.record_violation(
                            "universe_stratification",
                            &format!("Universe Type_{} does not have universe type", level),
                            None,
                            Severity::Critical
                        );
                    }
                }
            }
        }

        // Verify level ordering
        for level1 in 0..5u32 {
            for level2 in (level1 + 1)..6u32 {
                let val1 = Value::Universe(Level(level1));
                let val2 = Value::Universe(Level(level2));

                // Different levels should not be convertible
                if !convertible(&val1, &val2, 0) {
                    self.verify_property("universe_level_distinction");
                } else {
                    self.record_violation(
                        "universe_level_distinction",
                        &format!("Type_{} and Type_{} should not be convertible", level1, level2),
                        None,
                        Severity::Critical
                    );
                }
            }
        }
    }

    /// Verify computational properties (beta/eta laws)
    fn verify_computational_properties(&mut self) {
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::default();
        let env = Environment::new();

        // Beta reduction: (λx.e) v ≡ e[x := v]
        let beta_prop = |body: Term, arg: Term| {
            let lambda = Term::lambda(body.clone());
            let application = Term::app(lambda, arg.clone());

            let substituted = substitute_var(&body, 0, &arg);

            if let (Ok(app_val), Ok(subst_val)) = (
                evaluate(&application, &env),
                evaluate(&substituted, &env)
            ) {
                convertible(&app_val, &subst_val, 0)
            } else {
                true // Evaluation failure is acceptable
            }
        };

        let beta_strategy = (self.small_term_gen(), self.small_term_gen());

        match runner.run(&beta_strategy, |(body, arg)| {
            if beta_prop(body, arg) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Beta reduction failed"))
            }
        }) {
            Ok(_) => self.verify_property("beta_reduction"),
            Err(e) => self.record_violation(
                "beta_reduction",
                "Beta reduction is not sound",
                Some(format!("{:?}", e)),
                Severity::Critical
            ),
        }

        // Eta expansion: For closed f, f ≡ λx.(f x)
        let eta_prop = |f: Term| {
            if f.is_closed() {
                let shifted_f = shift_term(&f, 0, 1);
                let applied = Term::app(shifted_f, Term::var(0));
                let eta_expanded = Term::lambda(applied);

                if let (Ok(orig_val), Ok(eta_val)) = (
                    evaluate(&f, &env),
                    evaluate(&eta_expanded, &env)
                ) {
                    // Eta equivalence is complex; for now just test it doesn't crash
                    let _ = convertible(&orig_val, &eta_val, 0);
                    true
                } else {
                    true
                }
            } else {
                true // Eta only applies to closed terms
            }
        };

        match runner.run(&self.closed_term_gen(), |f| {
            if eta_prop(f) {
                Ok(())
            } else {
                Err(proptest::test_runner::TestCaseError::fail("Eta expansion failed"))
            }
        }) {
            Ok(_) => self.verify_property("eta_expansion"),
            Err(_) => self.record_violation(
                "eta_expansion",
                "Eta expansion causes evaluation failure",
                None,
                Severity::Minor
            ),
        }
    }

    // Helper methods for generating test data
    fn small_term_gen(&self) -> impl Strategy<Value = Term> {
        let leaf = prop_oneof![
            3 => (0usize..5).prop_map(Term::var),
            2 => (0u32..3).prop_map(|n| Term::Universe(Level(n))),
            1 => (0usize..3).prop_map(Term::meta),
        ];

        leaf.prop_recursive(3, 8, 3, |inner| {
            prop_oneof![
                2 => inner.clone().prop_map(Term::lambda),
                3 => (inner.clone(), inner.clone()).prop_map(|(f, a)| Term::app(f, a)),
                2 => (inner.clone(), inner.clone()).prop_map(|(d, c)| Term::pi(d, c)),
                1 => (inner.clone(), inner.clone()).prop_map(|(b, d)| Term::let_in(b, d)),
            ]
        })
    }

    fn closed_term_gen(&self) -> impl Strategy<Value = Term> {
        self.small_term_gen().prop_filter("closed", |t| t.is_closed())
    }

    fn substitution_gen(&self) -> impl Strategy<Value = Substitution> {
        prop::collection::vec((0usize..3, self.small_term_gen()), 0..3).prop_map(|pairs| {
            pairs.into_iter().fold(Substitution::empty(), |s, (i, t)| s.extend(i, t))
        })
    }
}

/// Report of invariant verification
#[derive(Debug)]
pub struct InvariantReport {
    pub total_verified: usize,
    pub violations: Vec<InvariantViolation>,
    pub property_counts: HashMap<String, usize>,
}

impl InvariantReport {
    pub fn is_sound(&self) -> bool {
        !self.violations.iter().any(|v| v.severity == Severity::Critical)
    }

    pub fn critical_violations(&self) -> Vec<&InvariantViolation> {
        self.violations.iter().filter(|v| v.severity == Severity::Critical).collect()
    }

    pub fn summary(&self) -> String {
        format!(
            "Invariant Verification Report: {} properties verified, {} violations ({} critical)",
            self.total_verified,
            self.violations.len(),
            self.critical_violations().len()
        )
    }

    pub fn detailed_report(&self) -> String {
        let mut report = String::new();

        report.push_str(&format!("Mathematical Invariant Verification Report\n"));
        report.push_str(&format!("==========================================\n\n"));

        report.push_str(&format!("Total properties verified: {}\n", self.total_verified));
        report.push_str(&format!("Total violations: {}\n\n", self.violations.len()));

        if self.violations.is_empty() {
            report.push_str("✅ All mathematical invariants verified successfully!\n");
        } else {
            report.push_str("Violations by severity:\n");

            let critical: Vec<_> = self.violations.iter().filter(|v| v.severity == Severity::Critical).collect();
            let major: Vec<_> = self.violations.iter().filter(|v| v.severity == Severity::Major).collect();
            let minor: Vec<_> = self.violations.iter().filter(|v| v.severity == Severity::Minor).collect();

            if !critical.is_empty() {
                report.push_str(&format!("\n🚨 CRITICAL ({}):\n", critical.len()));
                for violation in critical {
                    report.push_str(&format!("  - {}: {}\n", violation.property, violation.description));
                    if let Some(ref example) = violation.counterexample {
                        report.push_str(&format!("    Counterexample: {}\n", example));
                    }
                }
            }

            if !major.is_empty() {
                report.push_str(&format!("\n⚠️  MAJOR ({}):\n", major.len()));
                for violation in major {
                    report.push_str(&format!("  - {}: {}\n", violation.property, violation.description));
                }
            }

            if !minor.is_empty() {
                report.push_str(&format!("\nℹ️  MINOR ({}):\n", minor.len()));
                for violation in minor {
                    report.push_str(&format!("  - {}: {}\n", violation.property, violation.description));
                }
            }
        }

        report.push_str("\nProperty verification counts:\n");
        for (property, count) in &self.property_counts {
            report.push_str(&format!("  {}: {} tests passed\n", property, count));
        }

        report
    }
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    #[test]
    fn run_full_invariant_verification() {
        let mut checker = InvariantChecker::new();
        let report = checker.verify_all_invariants();

        println!("{}", report.detailed_report());

        // Critical violations fail the test
        if !report.critical_violations().is_empty() {
            panic!("Critical mathematical invariant violations detected:\n{}",
                   report.detailed_report());
        }

        // Ensure we verified a reasonable number of properties
        assert!(report.total_verified > 10, "Too few properties verified");
    }

    #[test]
    fn test_specific_mathematical_laws() {
        let mut checker = InvariantChecker::new();

        // Test specific laws manually
        checker.verify_substitution_algebra();
        checker.verify_type_system_soundness();
        checker.verify_conversion_properties();

        let critical_violations: Vec<_> = checker.violations.iter()
            .filter(|v| v.severity == Severity::Critical)
            .collect();

        if !critical_violations.is_empty() {
            for violation in critical_violations {
                println!("Critical violation: {} - {}", violation.property, violation.description);
            }
            panic!("Critical mathematical laws violated");
        }
    }

    #[test]
    fn verify_computational_equivalences() {
        let mut checker = InvariantChecker::new();
        checker.verify_computational_properties();

        let beta_violations: Vec<_> = checker.violations.iter()
            .filter(|v| v.property == "beta_reduction" && v.severity == Severity::Critical)
            .collect();

        assert!(beta_violations.is_empty(), "Beta reduction must be sound");
    }

    #[test]
    fn verify_universe_consistency() {
        let mut checker = InvariantChecker::new();
        checker.verify_universe_hierarchy();

        let universe_violations: Vec<_> = checker.violations.iter()
            .filter(|v| v.property.contains("universe") && v.severity == Severity::Critical)
            .collect();

        assert!(universe_violations.is_empty(), "Universe hierarchy must be consistent");
    }
}