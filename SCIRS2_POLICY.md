# SCIRS2 Policy Compliance Document

**Project**: rs3gw - High-Performance AI/HPC Object Storage Gateway
**Author/Copyright**: COOLJAPAN OU (Team Kitasan)
**Last Updated**: 2026-01-03
**Policy Version**: 1.0

---

## Overview

This document outlines rs3gw's compliance with the SCIRS2 (Scientific Rust Ecosystem) policies and best practices. The SCIRS2 policy ensures that the rs3gw project follows scientific computing standards, maintains pure Rust implementations where possible, and integrates seamlessly with the broader SciRS2 ecosystem of libraries.

## SciRS2 Ecosystem Integration

### Core Dependencies

Rs3gw integrates with the following SciRS2 ecosystem components:

#### scirs2-core (v0.1)
**Purpose**: Core scientific computing primitives and random number generation

**Usage in rs3gw**:
- **Random Number Generation**: Complete replacement for `rand` crate
  - `scirs2_core::random::quick::random_f64()` - Fast random float generation
  - `scirs2_core::random::quick::random_int()` - Integer range generation
  - `scirs2_core::random::quick::random_usize()` - Size type generation
- **Location**: Used in data augmentation, benchmarks, and test data generation
- **Benefits**:
  - Consistent RNG across SciRS2 ecosystem
  - Scientific-grade statistical distributions
  - Zero-copy optimizations for large datasets
  - Better reproducibility for scientific workloads

**Files using scirs2-core**:
- `src/storage/preprocessing.rs` - Image augmentation and data preprocessing
- `src/bin/testdata_generator.rs` - Test dataset generation
- `benches/compression_benchmarks.rs` - Performance benchmarking

#### scirs2-io (v0.1)
**Purpose**: High-performance I/O operations for scientific data formats

**Usage in rs3gw**:
- Scientific data format detection and handling
- Optimized buffering strategies for large datasets
- Zero-copy I/O operations where applicable

---

## Policy Compliance Status

### 1. No rand/rand_distr Policy ✅ COMPLIANT

**Policy**: Use SciRS2-Core instead of `rand` or `rand_distr` crates

**Status**: ✅ **FULLY COMPLIANT**

**Details**:
- All `rand` usage has been migrated to `scirs2_core::random::quick`
- Zero remaining direct `rand` dependencies in production code
- `getrandom` crate used only for secure random byte generation (cryptographic purposes)

**Migration Summary**:
| File | Old (rand) | New (scirs2-core) |
|------|-----------|-------------------|
| preprocessing.rs | `rand::rng().random()` | `scirs2_core::random::quick::random_f64()` |
| testdata_generator.rs | `rand::rng().random_range()` | `scirs2_core::random::quick::random_int()` |
| compression_benchmarks.rs | `rng.random()` | `random_f64() * 256.0` |

### 2. No ndarray Policy ✅ COMPLIANT

**Policy**: Use SciRS2-Core's array primitives instead of direct `ndarray` usage

**Status**: ✅ **COMPLIANT**

**Details**:
- No direct `ndarray` dependency in Cargo.toml
- SciRS2-Core provides array functionality where needed
- Arrow and Parquet crates use their own array representations (acceptable for format-specific operations)

### 3. COOLJAPAN Policy ✅ COMPLIANT

**Policy**:
- No `openblas` (use `oxiblas` instead)
- No `bincode` (use `oxicode` instead)

**Status**: ✅ **FULLY COMPLIANT**

**Details**:
- **openblas**: Zero occurrences in dependency tree ✓
- **bincode**: Zero occurrences in dependency tree ✓
- **oxiblas**: Not needed (no BLAS operations required)
- **oxicode**: Available if binary serialization needed (currently using standard formats)

**Verification Command**:
```bash
cargo tree --all-features | grep -i "openblas\|bincode"
# Result: No matches found
```

### 4. Pure Rust Policy ✅ COMPLIANT

**Policy**: Default features must be 100% Pure Rust. C/Fortran dependencies must be feature-gated.

**Status**: ✅ **FULLY COMPLIANT**

**Default Features**:
- ✅ 100% Pure Rust
- No C dependencies
- No Fortran dependencies
- No system library requirements (except libc for platform APIs)

**Feature-Gated Non-Rust Dependencies**:
| Feature | Dependency | Language | Reason |
|---------|-----------|----------|---------|
| `video-transcoding` | ffmpeg-next | C (FFmpeg) | Video encoding/decoding |
| `io_uring` | tokio-uring | C (liburing) | Linux-specific async I/O |

**Verification**:
```bash
# Default build (Pure Rust)
cargo build
# No C/Fortran compilation

# Feature-gated build
cargo build --features video-transcoding
# FFmpeg C library compiled
```

### 5. Workspace Policy ✅ COMPLIANT

**Policy**: Use Cargo workspace with `*.workspace = true` for dependency management

**Status**: ✅ **FULLY COMPLIANT**

**Details**:
- Root `Cargo.toml` defines workspace with all members
- All dependencies use `.workspace = true` pattern
- Version numbers defined once in `[workspace.dependencies]`
- No duplicate dependency specifications
- WASM plugin subcrates properly integrated

**Workspace Members**:
1. `.` (main rs3gw crate)
2. `examples/wasm-plugins/rust-uppercase`
3. `examples/wasm-plugins/uppercase-rust`
4. `examples/wasm-plugins/rust-json-filter`

**Benefits**:
- Single source of truth for versions
- Consistent dependency resolution
- Easier maintenance and updates
- Zero workspace-related warnings

### 6. No Warnings Policy ✅ COMPLIANT

**Policy**: Zero compiler warnings, zero clippy warnings

**Status**: ✅ **FULLY COMPLIANT**

**Verification Results**:
```bash
cargo build --all-features
# Result: 0 warnings

cargo clippy --all-features --all-targets
# Result: 0 warnings

cargo fmt --all --check
# Result: All files formatted correctly
```

### 7. No Unwrap Policy ✅ COMPLIANT

**Policy**: No `unwrap()` calls in production code (test code excluded)

**Status**: ✅ **FULLY COMPLIANT**

**Details**:
- All error handling uses proper `Result<T, E>` types
- No `unwrap()` calls in `src/` production code
- Test code may use `unwrap()` for clarity (acceptable)
- All fallible operations explicitly handle errors

**Verification**:
```bash
grep -rn "\.unwrap()" src/ --include="*.rs" | grep -v "// " | grep -v "test"
# Result: No matches found
```

### 8. Refactoring Policy ✅ COMPLIANT

**Policy**: Single files should not exceed 2,000 lines

**Status**: ✅ **FULLY COMPLIANT**

**File Size Statistics**:
| File | Lines | Status |
|------|-------|--------|
| src/api/handlers/functions.rs | 1,828 | ✅ Under limit |
| src/bin/rs3ctl.rs | 1,769 | ✅ Under limit |
| src/storage/mod.rs | 1,751 | ✅ Under limit |
| src/storage/intelligent_tiering.rs | 1,588 | ✅ Under limit |

**Largest File**: 1,828 lines (8.6% under 2,000 line limit)

### 9. Latest Crates Policy ✅ COMPLIANT

**Policy**: Use latest versions available on crates.io

**Status**: ✅ **FULLY COMPLIANT**

**Update Process**:
```bash
cargo update --dry-run
# Regular checks for new versions
# Dependencies updated to latest compatible versions
```

**Key Dependencies** (as of 2026-01-03):
- tokio: 1.48
- axum: 0.8.8
- serde: 1.0.228
- scirs2-core: 0.1
- scirs2-io: 0.1

### 10. IMPLEMENT Policy ✅ COMPLIANT

**Policy**: If something cannot be done because it's not implemented, try to implement it first regardless of scope

**Status**: ✅ **ACTIVE APPROACH**

**Examples of Implementation-First Approach**:
1. **S3 Select**: Instead of stubbing, implemented full SQL parser and executor
2. **gRPC API**: Implemented complete protocol buffer definitions and handlers
3. **ML Cache**: Built custom ML-based caching instead of using simple LRU
4. **Query Intelligence**: Implemented full index recommendation system
5. **Preprocessing Pipelines**: Built comprehensive image/video preprocessing framework

**Philosophy**: Build it right the first time rather than stub and defer

---

## SciRS2 Ecosystem Benefits

### 1. Scientific Computing Integration
- Seamless integration with other SciRS2 libraries
- Shared primitives and patterns across ecosystem
- Consistent behavior for scientific workloads

### 2. Performance Optimization
- Zero-copy operations where possible
- SIMD-optimized mathematical operations (via scirs2-core)
- Cache-friendly data structures

### 3. Reproducibility
- Deterministic random number generation
- Consistent numerical behavior across platforms
- Scientific-grade statistical distributions

### 4. Pure Rust Benefits
- Memory safety guarantees
- No C/FFI boundary overhead (in default features)
- Cross-platform portability
- Easier auditing and security

---

## Future SciRS2 Integration Plans

### Short Term (Next Release)

1. **scirs2-stats** integration for advanced analytics
   - Statistical analysis of access patterns
   - Anomaly detection improvements
   - Predictive modeling enhancements

2. **scirs2-linalg** for matrix operations
   - Advanced query optimization using linear algebra
   - Dimensionality reduction for cache optimization

### Medium Term (6-12 months)

1. **scirs2-neural** for ML model serving
   - Direct integration with neural network models
   - Model format conversion
   - Inference optimization

2. **scirs2-transform** for data transformations
   - Scientific data preprocessing
   - Format conversions
   - Feature extraction pipelines

### Long Term (12+ months)

1. **Full SciRS2 Ecosystem Integration**
   - Integration with all major SciRS2 components
   - Become reference implementation for object storage in SciRS2
   - Contribute improvements back to SciRS2 ecosystem

---

## Compliance Verification

### Automated Checks

We maintain automated verification of policy compliance:

```bash
# Run all compliance checks
./scripts/verify_policies.sh

# Individual checks
cargo build --no-default-features  # Verify Pure Rust
cargo clippy --all-targets        # Verify No Warnings
cargo test --all-features          # Verify all tests pass
tokei .                            # Verify file sizes
```

### Continuous Integration

All policies are verified in CI/CD:
- Pre-commit hooks check formatting and clippy
- CI builds test all feature combinations
- Nightly builds check for dependency updates
- Weekly policy compliance reports

---

## Contact and Contributions

**Organization**: COOLJAPAN OU (Team Kitasan)
**Repository**: https://github.com/cool-japan/rs3gw
**SciRS2 Ecosystem**: https://github.com/cool-japan/scirs

### Contributing to SciRS2 Compliance

When contributing to rs3gw, please ensure:

1. No introduction of `rand` or `ndarray` direct dependencies
2. Use `scirs2_core::random` for all RNG needs
3. Maintain workspace dependency pattern
4. Zero warnings policy maintained
5. All new code follows Pure Rust policy (feature-gate if needed)
6. File sizes stay under 2,000 lines
7. Proper error handling (no `unwrap()` in production)

---

## Appendix A: Migration Guide

### Migrating from rand to scirs2-core

**Before**:
```rust
use rand::Rng;
let mut rng = rand::rng();
let x = rng.random::<f64>();
let y = rng.random_range(0..100);
```

**After**:
```rust
use scirs2_core::random::quick::{random_f64, random_int};
let x = random_f64();
let y = random_int(0, 99);
```

### Migrating to Workspace Dependencies

**Before** (in subcrate Cargo.toml):
```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
```

**After**:
```toml
[dependencies]
serde.workspace = true
```

(with `serde = { version = "1.0", features = ["derive"] }` defined in root workspace)

---

## Appendix B: Related Projects

Rs3gw is part of the broader COOLJAPAN OU scientific Rust ecosystem:

### Core Libraries
- **SciRS2**: Scientific computing core
- **NumRS2**: Numerical operations
- **ToRSh**: PyTorch-like tensor operations
- **TrustformeRS**: Transformer models
- **TenfloweRS**: TensorFlow-like operations

### Specialized Libraries
- **SkleaRS**: Scikit-learn equivalent
- **OptiRS**: Optimization algorithms
- **VoiRS**: Voice processing
- **OxiRS**: Various utilities
- **OxiBLAS**: Pure Rust BLAS
- **Oxicode**: Binary serialization

### Related to This Project
- **TensorLogic**: Tensor computation logic
- **TenRSo**: Tensor operations

All projects follow the same SCIRS2 policy compliance standards documented here.

---

**Document Version**: 1.0
**Last Review**: 2026-01-03
**Next Review**: 2026-04-03 (Quarterly)
