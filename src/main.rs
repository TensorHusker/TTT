//! TTT - Tiny Type Theory CLI

extern crate alloc;

use ttt::{Term, TypeChecker, TypeEnv, Normalizer};
use alloc::sync::Arc;

fn main() {
    println!("TTT - Tiny Type Theory v0.1.0");
    println!("A categorical kernel of dependent types\n");
    
    // Demonstrate basic functionality
    demo_identity_function();
    demo_pair_operations();
    demo_normalization();
    demo_universe_hierarchy();
}

fn demo_identity_function() {
    println!("=== Identity Function Demo ===");
    
    let mut checker = TypeChecker::new(10);
    let mut env = TypeEnv::new();
    
    // A : Type₀
    let type0 = Term::universe(0);
    let a_level = env.extend(type0.clone());
    println!("A : Type₀  (level {})", a_level);
    
    // x : A  
    let var_a = Term::var(a_level);
    let x_level = env.extend(var_a.clone());
    println!("x : A  (level {})", x_level);
    
    // λx. x : A -> A
    let var_x = Term::var(x_level);
    let identity_body = Term::lambda(None, var_x);
    let identity_type = Term::pi(None, var_a.clone(), var_a);
    
    println!("Checking: {} : {}", identity_body, identity_type);
    
    match checker.check(&env, &identity_body, &identity_type) {
        Ok(_) => println!("✓ Type check passed!\n"),
        Err(e) => println!("✗ Type check failed: {}\n", e),
    }
}

fn demo_pair_operations() {
    println!("=== Pair Operations Demo ===");
    
    let mut checker = TypeChecker::new(10);
    let env = TypeEnv::new();
    
    let type0 = Term::universe(0);
    
    // (Type₀, Type₀) : Type₀ × Type₀
    let pair = Term::pair(type0.clone(), type0.clone());
    let pair_type = Term::sigma(None, type0.clone(), type0.clone());
    
    println!("Checking: {} : {}", pair, pair_type);
    
    match checker.check(&env, &pair, &pair_type) {
        Ok(_) => println!("✓ Pair type check passed!"),
        Err(e) => println!("✗ Pair type check failed: {}", e),
    }
    
    // Test projections
    let fst_proj = Term::fst(pair.clone());
    println!("First projection: {}", fst_proj);
    
    match checker.infer(&env, &fst_proj) {
        Ok(ty) => println!("✓ Inferred type: {}", ty),
        Err(e) => println!("✗ Inference failed: {}", e),
    }
    
    println!();
}

fn demo_normalization() {
    println!("=== Normalization Demo ===");
    
    let mut normalizer = Normalizer::new();
    
    // (λx. x) Type₀
    let identity = Term::lambda(None, Term::var(0));
    let type0 = Term::universe(0);
    let application = Term::app(identity.clone(), type0.clone());
    
    println!("Original: {}", application);
    
    let normalized = normalizer.normalize(&application);
    println!("Normalized: {}", normalized);
    
    // Test alpha equivalence
    let identity2 = Term::lambda(None, Term::var(0));
    println!("Alpha equivalent λx.x ≡ λy.y: {}", 
             normalizer.alpha_equiv(&identity, &identity2));
    
    println!();
}

fn demo_universe_hierarchy() {
    println!("=== Universe Hierarchy Demo ===");
    
    let mut checker = TypeChecker::new(3);
    let env = TypeEnv::new();
    
    for level in 0..4 {
        let universe = Term::universe(level);
        println!("Type{}: ", level);
        
        match checker.infer(&env, &universe) {
            Ok(ty) => println!("  ✓ : {}", ty),
            Err(e) => println!("  ✗ Error: {}", e),
        }
    }
    
    println!();
    println!("TTT kernel loaded successfully! 🎯");
}