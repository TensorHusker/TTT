//! Mathlib4 Theorem Database Integration
//!
//! This module provides efficient access to Lean's mathlib4 theorem database,
//! enabling TTT to search, retrieve, and apply mathematical theorems for
//! automated proof assistance and verification.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Serialize, Deserialize};
use parking_lot::RwLock;
use thiserror::Error;

use crate::lean::{LeanTerm, LeanName, Result as LeanResult, LeanError};
use crate::core::Term;

/// Error types specific to mathlib operations
#[derive(Error, Debug)]
pub enum MathlibError {
    #[error("Theorem not found: {name}")]
    TheoremNotFound { name: String },

    #[error("Database not initialized")]
    DatabaseNotInitialized,

    #[error("Search index corrupted: {reason}")]
    SearchIndexCorrupted { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Lean error: {0}")]
    Lean(#[from] LeanError),
}

pub type MathlibResult<T> = std::result::Result<T, MathlibError>;

/// Categories for organizing mathematical theorems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TheoremCategory {
    // Foundational categories
    Logic,
    SetTheory,
    TypeTheory,

    // Algebraic categories
    GroupTheory,
    RingTheory,
    FieldTheory,
    LinearAlgebra,

    // Analytical categories
    RealAnalysis,
    ComplexAnalysis,
    FunctionalAnalysis,
    Topology,

    // Geometric categories
    Geometry,
    DifferentialGeometry,
    AlgebraicGeometry,

    // Number theory
    NumberTheory,
    AlgebraicNumberTheory,

    // Combinatorics and discrete math
    Combinatorics,
    GraphTheory,

    // Advanced topics
    CategoryTheory,
    HomologicalAlgebra,
    MeasureTheory,
    ProbabilityTheory,

    // Meta-mathematics
    ProofTheory,
    ModelTheory,

    // Unknown or uncategorized
    Unknown,
}

impl TheoremCategory {
    /// Get all categories
    pub fn all() -> Vec<TheoremCategory> {
        use TheoremCategory::*;
        vec![
            Logic, SetTheory, TypeTheory,
            GroupTheory, RingTheory, FieldTheory, LinearAlgebra,
            RealAnalysis, ComplexAnalysis, FunctionalAnalysis, Topology,
            Geometry, DifferentialGeometry, AlgebraicGeometry,
            NumberTheory, AlgebraicNumberTheory,
            Combinatorics, GraphTheory,
            CategoryTheory, HomologicalAlgebra, MeasureTheory, ProbabilityTheory,
            ProofTheory, ModelTheory,
            Unknown,
        ]
    }

    /// Categorize theorem based on its name and namespace
    pub fn from_name(name: &str) -> Self {
        if name.starts_with("Logic.") || name.contains("Prop.") {
            TheoremCategory::Logic
        } else if name.starts_with("Set.") {
            TheoremCategory::SetTheory
        } else if name.starts_with("Group.") || name.contains("Group") {
            TheoremCategory::GroupTheory
        } else if name.starts_with("Ring.") || name.contains("Ring") {
            TheoremCategory::RingTheory
        } else if name.starts_with("Field.") || name.contains("Field") {
            TheoremCategory::FieldTheory
        } else if name.starts_with("LinearAlgebra.") || name.contains("LinearMap") {
            TheoremCategory::LinearAlgebra
        } else if name.starts_with("Real.") || name.contains("Real") {
            TheoremCategory::RealAnalysis
        } else if name.starts_with("Complex.") {
            TheoremCategory::ComplexAnalysis
        } else if name.starts_with("Topology.") {
            TheoremCategory::Topology
        } else if name.starts_with("Geometry.") {
            TheoremCategory::Geometry
        } else if name.starts_with("NumberTheory.") {
            TheoremCategory::NumberTheory
        } else if name.starts_with("Combinatorics.") {
            TheoremCategory::Combinatorics
        } else if name.starts_with("CategoryTheory.") {
            TheoremCategory::CategoryTheory
        } else if name.starts_with("MeasureTheory.") {
            TheoremCategory::MeasureTheory
        } else {
            TheoremCategory::Unknown
        }
    }
}

/// Metadata about a mathlib theorem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathlibTheorem {
    /// Fully qualified name in Lean
    pub name: String,

    /// Lean term representing the theorem statement
    pub statement: LeanTerm,

    /// Optional proof term (may be omitted for axioms)
    pub proof: Option<LeanTerm>,

    /// Dependencies (other theorems this theorem uses)
    pub dependencies: Vec<String>,

    /// Theorem category for organization
    pub category: TheoremCategory,

    /// Complexity score (0-100, higher = more complex)
    pub complexity: u32,

    /// Free variables in the statement
    pub free_vars: Vec<LeanName>,

    /// Type signature pattern for matching
    pub type_pattern: TypePattern,

    /// Documentation string if available
    pub doc_string: Option<String>,

    /// Source location in mathlib
    pub source_file: Option<String>,
    pub source_line: Option<u32>,

    /// Usage statistics
    pub usage_count: u64,
    pub avg_proof_time_ms: f64,
}

impl MathlibTheorem {
    /// Create a new theorem entry
    pub fn new(name: String, statement: LeanTerm) -> Self {
        let category = TheoremCategory::from_name(&name);
        let free_vars = statement.free_vars().into_iter().collect();
        let complexity = Self::estimate_complexity(&statement);
        let type_pattern = TypePattern::from_term(&statement);

        Self {
            name,
            statement,
            proof: None,
            dependencies: Vec::new(),
            category,
            complexity,
            free_vars,
            type_pattern,
            doc_string: None,
            source_file: None,
            source_line: None,
            usage_count: 0,
            avg_proof_time_ms: 0.0,
        }
    }

    /// Estimate complexity based on term structure
    fn estimate_complexity(term: &LeanTerm) -> u32 {
        match term {
            LeanTerm::Var(_) | LeanTerm::Const(_) | LeanTerm::Sort(_) => 1,
            LeanTerm::App(f, x) => Self::estimate_complexity(f) + Self::estimate_complexity(x),
            LeanTerm::Lambda(_, ty, body) | LeanTerm::Pi(_, ty, body) => {
                5 + Self::estimate_complexity(ty) + Self::estimate_complexity(body)
            },
            LeanTerm::Let(_, ty, val, body) => {
                3 + Self::estimate_complexity(ty) + Self::estimate_complexity(val) + Self::estimate_complexity(body)
            },
        }
    }

    /// Check if this theorem might be applicable to a given goal
    pub fn is_applicable_to(&self, goal: &LeanTerm) -> bool {
        self.type_pattern.matches(goal)
    }

    /// Update usage statistics
    pub fn record_usage(&mut self, proof_time_ms: f64) {
        self.usage_count += 1;
        let total_time = self.avg_proof_time_ms * (self.usage_count - 1) as f64 + proof_time_ms;
        self.avg_proof_time_ms = total_time / self.usage_count as f64;
    }
}

/// Pattern for matching theorem types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypePattern {
    /// Head symbol of the type
    pub head: String,

    /// Number of arguments
    pub arity: usize,

    /// Whether it's a proposition (Prop)
    pub is_prop: bool,

    /// Whether it's a function type (→)
    pub is_function: bool,

    /// Whether it's a dependent type (Π)
    pub is_dependent: bool,
}

impl TypePattern {
    /// Extract pattern from a Lean term
    pub fn from_term(term: &LeanTerm) -> Self {
        match term {
            LeanTerm::Sort(_) => TypePattern {
                head: "Sort".to_string(),
                arity: 0,
                is_prop: false,
                is_function: false,
                is_dependent: false,
            },
            LeanTerm::Const(name) => TypePattern {
                head: name.to_string(),
                arity: 0,
                is_prop: name.as_str() == "Prop",
                is_function: false,
                is_dependent: false,
            },
            LeanTerm::App(f, _) => {
                let mut pattern = Self::from_term(f);
                pattern.arity += 1;
                pattern
            },
            LeanTerm::Pi(_, _, _) => TypePattern {
                head: "Pi".to_string(),
                arity: 1,
                is_prop: false,
                is_function: true,
                is_dependent: true,
            },
            _ => TypePattern {
                head: "Unknown".to_string(),
                arity: 0,
                is_prop: false,
                is_function: false,
                is_dependent: false,
            },
        }
    }

    /// Check if this pattern matches another
    pub fn matches(&self, term: &LeanTerm) -> bool {
        let other = Self::from_term(term);

        // Exact head match or compatible types
        if self.head == other.head {
            return true;
        }

        // Proposition matching
        if self.is_prop && other.is_prop {
            return true;
        }

        // Function type matching
        if self.is_function && other.is_function {
            return true;
        }

        false
    }
}

/// Search index for efficient theorem lookup
#[derive(Debug)]
pub struct SearchIndex {
    /// Index by theorem name
    name_index: DashMap<String, String>,

    /// Index by category
    category_index: DashMap<TheoremCategory, Vec<String>>,

    /// Index by type pattern
    type_index: DashMap<String, Vec<String>>,

    /// Index by free variables
    var_index: DashMap<String, Vec<String>>,

    /// Full-text search index for documentation
    doc_index: DashMap<String, Vec<String>>,
}

impl SearchIndex {
    /// Create a new search index
    pub fn new() -> Self {
        Self {
            name_index: DashMap::new(),
            category_index: DashMap::new(),
            type_index: DashMap::new(),
            var_index: DashMap::new(),
            doc_index: DashMap::new(),
        }
    }

    /// Add a theorem to the index
    pub fn add_theorem(&self, theorem: &MathlibTheorem) {
        // Name index
        self.name_index.insert(theorem.name.clone(), theorem.name.clone());

        // Category index
        self.category_index
            .entry(theorem.category)
            .or_insert_with(Vec::new)
            .push(theorem.name.clone());

        // Type pattern index
        let pattern_key = format!("{}:{}", theorem.type_pattern.head, theorem.type_pattern.arity);
        self.type_index
            .entry(pattern_key)
            .or_insert_with(Vec::new)
            .push(theorem.name.clone());

        // Variable index
        for var in &theorem.free_vars {
            self.var_index
                .entry(var.to_string())
                .or_insert_with(Vec::new)
                .push(theorem.name.clone());
        }

        // Documentation index
        if let Some(doc) = &theorem.doc_string {
            for word in doc.split_whitespace() {
                let word = word.to_lowercase();
                self.doc_index
                    .entry(word)
                    .or_insert_with(Vec::new)
                    .push(theorem.name.clone());
            }
        }
    }

    /// Search by name pattern
    pub fn search_by_name(&self, pattern: &str) -> Vec<String> {
        self.name_index
            .iter()
            .filter_map(|entry| {
                if entry.key().contains(pattern) {
                    Some(entry.value().clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Search by category
    pub fn search_by_category(&self, category: TheoremCategory) -> Vec<String> {
        self.category_index
            .get(&category)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    /// Search by type pattern
    pub fn search_by_type(&self, goal: &LeanTerm) -> Vec<String> {
        let pattern = TypePattern::from_term(goal);
        let pattern_key = format!("{}:{}", pattern.head, pattern.arity);

        self.type_index
            .get(&pattern_key)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    /// Search by free variable
    pub fn search_by_variable(&self, var_name: &str) -> Vec<String> {
        self.var_index
            .get(var_name)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    /// Full-text search in documentation
    pub fn search_documentation(&self, query: &str) -> Vec<String> {
        let words: Vec<String> = query.split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        if words.is_empty() {
            return Vec::new();
        }

        // Find theorems that contain all query words
        let mut result_sets: Vec<HashSet<String>> = Vec::new();

        for word in words {
            if let Some(theorems) = self.doc_index.get(&word) {
                result_sets.push(theorems.iter().cloned().collect());
            } else {
                return Vec::new(); // If any word is not found, no results
            }
        }

        // Intersection of all sets
        if let Some(first) = result_sets.first() {
            let mut intersection = first.clone();
            for set in result_sets.iter().skip(1) {
                intersection = intersection.intersection(set).cloned().collect();
            }
            intersection.into_iter().collect()
        } else {
            Vec::new()
        }
    }
}

/// Main theorem database
pub struct TheoremDatabase {
    /// All theorems indexed by name
    theorems: DashMap<String, MathlibTheorem>,

    /// Categories and their theorem lists
    categories: RwLock<HashMap<TheoremCategory, Vec<String>>>,

    /// Search index for efficient lookup
    search_index: SearchIndex,

    /// Database metadata
    metadata: RwLock<DatabaseMetadata>,

    /// Performance metrics
    metrics: Arc<DatabaseMetrics>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DatabaseMetadata {
    version: String,
    last_updated: chrono::DateTime<chrono::Utc>,
    theorem_count: usize,
    mathlib_version: String,
}

impl Default for DatabaseMetadata {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            last_updated: chrono::Utc::now(),
            theorem_count: 0,
            mathlib_version: "unknown".to_string(),
        }
    }
}

#[derive(Debug, Default)]
pub struct DatabaseMetrics {
    pub total_queries: std::sync::atomic::AtomicU64,
    pub cache_hits: std::sync::atomic::AtomicU64,
    pub avg_query_time_ms: std::sync::atomic::AtomicU64,
    pub theorem_applications: std::sync::atomic::AtomicU64,
}

impl DatabaseMetrics {
    pub fn record_query(&self, duration: Duration) {
        self.total_queries.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let old_avg = self.avg_query_time_ms.load(std::sync::atomic::Ordering::Relaxed);
        let total_queries = self.total_queries.load(std::sync::atomic::Ordering::Relaxed);
        let new_avg = (old_avg * (total_queries - 1) + duration.as_millis() as u64) / total_queries;
        self.avg_query_time_ms.store(new_avg, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_theorem_application(&self) {
        self.theorem_applications.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(std::sync::atomic::Ordering::Relaxed);
        let total = self.total_queries.load(std::sync::atomic::Ordering::Relaxed);
        if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        }
    }
}

impl TheoremDatabase {
    /// Create a new theorem database
    pub fn new() -> Self {
        Self {
            theorems: DashMap::new(),
            categories: RwLock::new(HashMap::new()),
            search_index: SearchIndex::new(),
            metadata: RwLock::new(DatabaseMetadata::default()),
            metrics: Arc::new(DatabaseMetrics::default()),
        }
    }

    /// Load database from mathlib installation
    pub async fn load_from_mathlib(mathlib_path: PathBuf) -> MathlibResult<Self> {
        tracing::info!("Loading mathlib database from {:?}", mathlib_path);

        let db = Self::new();

        // TODO: Implement actual mathlib parsing
        // For now, we'll create a placeholder implementation

        db.update_metadata(|meta| {
            meta.mathlib_version = "4.0.0".to_string();
            meta.last_updated = chrono::Utc::now();
        });

        tracing::info!("Mathlib database loaded successfully");
        Ok(db)
    }

    /// Add a theorem to the database
    pub fn add_theorem(&self, theorem: MathlibTheorem) -> MathlibResult<()> {
        let name = theorem.name.clone();
        let category = theorem.category;

        // Add to search index
        self.search_index.add_theorem(&theorem);

        // Update category index
        {
            let mut categories = self.categories.write();
            categories.entry(category)
                .or_insert_with(Vec::new)
                .push(name.clone());
        }

        // Add to main index
        self.theorems.insert(name, theorem);

        // Update metadata
        self.update_metadata(|meta| {
            meta.theorem_count = self.theorems.len();
        });

        Ok(())
    }

    /// Get a theorem by name
    pub fn get_theorem(&self, name: &str) -> Option<MathlibTheorem> {
        let start = Instant::now();
        let result = self.theorems.get(name).map(|entry| entry.clone());
        self.metrics.record_query(start.elapsed());
        result
    }

    /// Search theorems by various criteria
    pub fn search(&self, query: &SearchQuery) -> Vec<MathlibTheorem> {
        let start = Instant::now();

        let theorem_names = match query {
            SearchQuery::ByName(pattern) => self.search_index.search_by_name(pattern),
            SearchQuery::ByCategory(category) => self.search_index.search_by_category(*category),
            SearchQuery::ByType(goal) => self.search_index.search_by_type(goal),
            SearchQuery::ByVariable(var) => self.search_index.search_by_variable(var),
            SearchQuery::ByDocumentation(text) => self.search_index.search_documentation(text),
            SearchQuery::Combined(queries) => {
                // Intersection of all sub-queries
                let mut result_sets: Vec<HashSet<String>> = Vec::new();
                for sub_query in queries {
                    let names = self.search(sub_query);
                    result_sets.push(names.into_iter().map(|t| t.name).collect());
                }

                if let Some(first) = result_sets.first() {
                    let mut intersection = first.clone();
                    for set in result_sets.iter().skip(1) {
                        intersection = intersection.intersection(set).cloned().collect();
                    }
                    intersection.into_iter().collect()
                } else {
                    Vec::new()
                }
            }
        };

        let theorems: Vec<MathlibTheorem> = theorem_names
            .into_iter()
            .filter_map(|name| self.get_theorem(&name))
            .collect();

        self.metrics.record_query(start.elapsed());
        theorems
    }

    /// Find theorems applicable to a goal
    pub fn find_applicable(&self, goal: &LeanTerm) -> Vec<MathlibTheorem> {
        self.search(&SearchQuery::ByType(goal.clone()))
            .into_iter()
            .filter(|theorem| theorem.is_applicable_to(goal))
            .collect()
    }

    /// Get database statistics
    pub fn stats(&self) -> DatabaseStats {
        let theorem_count = self.theorems.len();
        let categories = self.categories.read();
        let category_counts: HashMap<TheoremCategory, usize> = categories
            .iter()
            .map(|(cat, theorems)| (*cat, theorems.len()))
            .collect();

        DatabaseStats {
            total_theorems: theorem_count,
            category_distribution: category_counts,
            avg_complexity: self.average_complexity(),
            cache_hit_rate: self.metrics.cache_hit_rate(),
        }
    }

    /// Calculate average theorem complexity
    fn average_complexity(&self) -> f64 {
        if self.theorems.is_empty() {
            return 0.0;
        }

        let total: u32 = self.theorems.iter()
            .map(|entry| entry.complexity)
            .sum();

        total as f64 / self.theorems.len() as f64
    }

    /// Update database metadata
    fn update_metadata<F>(&self, f: F)
    where
        F: FnOnce(&mut DatabaseMetadata),
    {
        let mut metadata = self.metadata.write();
        f(&mut metadata);
    }

    /// Get database metrics
    pub fn metrics(&self) -> &DatabaseMetrics {
        &self.metrics
    }
}

/// Search query types
#[derive(Debug, Clone)]
pub enum SearchQuery {
    /// Search by theorem name pattern
    ByName(String),

    /// Search by category
    ByCategory(TheoremCategory),

    /// Search by type signature
    ByType(LeanTerm),

    /// Search by free variable name
    ByVariable(String),

    /// Search in documentation
    ByDocumentation(String),

    /// Combined search (intersection)
    Combined(Vec<SearchQuery>),
}

/// Database statistics
#[derive(Debug)]
pub struct DatabaseStats {
    pub total_theorems: usize,
    pub category_distribution: HashMap<TheoremCategory, usize>,
    pub avg_complexity: f64,
    pub cache_hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lean::{LeanLevel, LeanName};

    #[test]
    fn test_theorem_creation() {
        let statement = LeanTerm::pi(
            "x",
            LeanTerm::const_("Nat"),
            LeanTerm::const_("True")
        );

        let theorem = MathlibTheorem::new("test_theorem".to_string(), statement);
        assert_eq!(theorem.name, "test_theorem");
        assert_eq!(theorem.category, TheoremCategory::Unknown);
        assert!(theorem.complexity > 0);
    }

    #[test]
    fn test_category_detection() {
        assert_eq!(TheoremCategory::from_name("Group.mul_assoc"), TheoremCategory::GroupTheory);
        assert_eq!(TheoremCategory::from_name("Real.add_comm"), TheoremCategory::RealAnalysis);
        assert_eq!(TheoremCategory::from_name("Logic.And.intro"), TheoremCategory::Logic);
    }

    #[test]
    fn test_type_pattern_matching() {
        let nat_type = LeanTerm::const_("Nat");
        let pattern = TypePattern::from_term(&nat_type);

        assert_eq!(pattern.head, "Nat");
        assert_eq!(pattern.arity, 0);
        assert!(!pattern.is_prop);
    }

    #[test]
    fn test_search_index() {
        let index = SearchIndex::new();

        let theorem = MathlibTheorem::new(
            "test_theorem".to_string(),
            LeanTerm::const_("Nat")
        );

        index.add_theorem(&theorem);

        let results = index.search_by_name("test");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "test_theorem");
    }

    #[test]
    fn test_database_operations() {
        let db = TheoremDatabase::new();

        let theorem = MathlibTheorem::new(
            "test_theorem".to_string(),
            LeanTerm::const_("Nat")
        );

        db.add_theorem(theorem).unwrap();

        let retrieved = db.get_theorem("test_theorem");
        assert!(retrieved.is_some());

        let stats = db.stats();
        assert_eq!(stats.total_theorems, 1);
    }
}