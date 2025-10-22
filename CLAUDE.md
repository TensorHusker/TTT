# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TTT (Tiny Type Theory) is a Rust implementation of the categorical kernel of dependent types, featuring Π/Σ/≡ types via zero-cost abstractions. This serves as a foundation for:
- Type-directed compression
- Proof-carrying optimization
- Verified systems
- Neural network verification
- Blockchain VMs
- Embedded proof checking

## Development Commands

This repository is in early development stage. Once the Rust project structure is established, common commands will be:

```bash
# Build the project
cargo build

# Run tests
cargo test

# Check code without building
cargo check

# Run with optimizations
cargo build --release

# Run a specific test
cargo test <test_name>

# Format code
cargo fmt

# Run clippy linter
cargo clippy
```

## Architecture Goals

Based on the project description, TTT aims to implement:

### Core Type System
- **Π types (Pi types)**: Dependent function types
- **Σ types (Sigma types)**: Dependent pair types
- **≡ types (Identity types)**: Equality/path types
- **Zero-cost abstractions**: Types that compile to efficient machine code

### Target Applications
- **Type-directed compression**: Using type information to optimize data representation
- **Proof-carrying optimization**: Optimizations with mathematical guarantees
- **Verified systems**: Systems with formal correctness proofs
- **Neural network verification**: Formal verification of ML models
- **Blockchain VMs**: Virtual machines with type-theoretic foundations
- **Embedded proof checking**: Lightweight proof verification for resource-constrained environments

## Development Guidelines

### Rust-Specific Considerations
- Leverage Rust's ownership system for memory safety in type theory implementation
- Use zero-cost abstractions to ensure type theory constructs don't add runtime overhead
- Consider `no_std` compatibility for embedded applications
- Design APIs that make dependent type operations ergonomic in Rust

### Type Theory Implementation
- Maintain mathematical rigor while ensuring practical performance
- Implement normalization and conversion checking efficiently
- Design for extensibility to support advanced type theory features
- Consider integration with proof assistants and theorem provers

### Testing Strategy
- Property-based testing for type theory laws and invariants
- Performance benchmarks to verify zero-cost abstraction claims
- Integration tests with target applications (compression, verification, etc.)
- Formal specification tests against type theory literature

## Project Status

This repository is in the initial setup phase. The first development tasks will likely involve:
1. Setting up basic Rust project structure with `cargo init`
2. Defining core type representations
3. Implementing basic type operations (formation, introduction, elimination)
4. Building normalization and conversion checking
5. Adding dependent type support
6. Optimizing for zero-cost abstractions

## Related Context

This project is part of a broader ecosystem including Runetika (an RPG that serves as an AI reasoning laboratory) and SCTT (Smooth Cubical Type Theory). TTT provides the foundational type theory that supports these higher-level applications.