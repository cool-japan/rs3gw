#!/usr/bin/env bash
#
# SCIRS2 Policy Compliance Verification Script
# Copyright: COOLJAPAN OU (Team Kitasan)
#
# This script verifies that rs3gw complies with all SCIRS2 policies
# Run this before committing code or in CI/CD pipelines

set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track overall success
OVERALL_SUCCESS=true

echo "======================================================================"
echo "  SCIRS2 Policy Compliance Verification"
echo "  Project: rs3gw"
echo "  Date: $(date)"
echo "======================================================================"
echo ""

# Function to print success
print_success() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
}

# Function to print failure
print_failure() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    OVERALL_SUCCESS=false
}

# Function to print warning
print_warning() {
    echo -e "${YELLOW}⚠ WARN${NC}: $1"
}

# 1. No Warnings Policy
echo "1. Checking No Warnings Policy..."
if cargo build --all-features 2>&1 | grep -i "warning:" > /dev/null; then
    print_failure "Compiler warnings found"
    cargo build --all-features 2>&1 | grep -i "warning:" | head -100
else
    print_success "No compiler warnings"
fi

if cargo clippy --all-features --all-targets 2>&1 | grep -i "warning:" > /dev/null; then
    print_failure "Clippy warnings found"
    cargo clippy --all-features --all-targets 2>&1 | grep -i "warning:" | head -100
else
    print_success "No clippy warnings"
fi
echo ""

# 2. No Unwrap Policy
echo "2. Checking No Unwrap Policy..."
UNWRAPS=$(grep -rn "\.unwrap()" src/ --include="*.rs" | grep -v "// " | grep -v "test" | wc -l)
if [ "$UNWRAPS" -eq 0 ]; then
    print_success "No unwrap() calls in production code"
else
    print_failure "Found $UNWRAPS unwrap() calls in production code"
    grep -rn "\.unwrap()" src/ --include="*.rs" | grep -v "// " | grep -v "test" | head -100
fi
echo ""

# 3. Refactoring Policy (files under 2000 lines)
echo "3. Checking Refactoring Policy (max 2000 lines per file)..."
LARGE_FILES=$(find src -name "*.rs" -type f -exec wc -l {} + | awk '$1 > 2000 {print}' | wc -l)
if [ "$LARGE_FILES" -eq 0 ]; then
    print_success "All files under 2000 lines"
else
    print_failure "Found files exceeding 2000 lines"
    find src -name "*.rs" -type f -exec wc -l {} + | awk '$1 > 2000 {print}' | head -100
fi
echo ""

# 4. SCIRS2 Policy (no rand, no ndarray direct deps)
echo "4. Checking SCIRS2 Policy (no rand/ndarray)..."
if grep -q "^rand = " Cargo.toml; then
    print_failure "Direct rand dependency found in Cargo.toml"
else
    print_success "No direct rand dependency"
fi

if grep -q "^ndarray = " Cargo.toml; then
    print_failure "Direct ndarray dependency found in Cargo.toml"
else
    print_success "No direct ndarray dependency"
fi

# Check for scirs2-core usage
if grep -q "scirs2-core" Cargo.toml; then
    print_success "scirs2-core dependency found"
else
    print_warning "scirs2-core dependency not found (should be using SciRS2 ecosystem)"
fi
echo ""

# 5. COOLJAPAN Policy (no openblas, no bincode)
echo "5. Checking COOLJAPAN Policy (no openblas/bincode)..."
OPENBLAS_COUNT=$(cargo tree --all-features 2>/dev/null | grep -i "openblas" | wc -l || echo "0")
BINCODE_COUNT=$(cargo tree --all-features 2>/dev/null | grep -i "bincode" | wc -l || echo "0")

if [ "$OPENBLAS_COUNT" -eq 0 ]; then
    print_success "No openblas in dependency tree"
else
    print_failure "Found openblas in dependency tree"
    cargo tree --all-features | grep -i "openblas" | head -100
fi

if [ "$BINCODE_COUNT" -eq 0 ]; then
    print_success "No bincode in dependency tree"
else
    print_failure "Found bincode in dependency tree"
    cargo tree --all-features | grep -i "bincode" | head -100
fi
echo ""

# 6. Pure Rust Policy (default features)
echo "6. Checking Pure Rust Policy (default features)..."
if cargo build --no-default-features 2>&1 | grep -E "Compiling.*-sys " > /dev/null; then
    print_failure "C dependencies found in default features"
else
    print_success "Default features are pure Rust"
fi
echo ""

# 7. Workspace Policy
echo "7. Checking Workspace Policy..."
if grep -q "\[workspace\]" Cargo.toml; then
    print_success "Workspace configuration found"
else
    print_failure "No workspace configuration in Cargo.toml"
fi

# Check for .workspace = true usage
WORKSPACE_DEPS=$(grep "\.workspace = true" Cargo.toml | wc -l)
if [ "$WORKSPACE_DEPS" -gt 50 ]; then
    print_success "Using workspace dependencies ($WORKSPACE_DEPS found)"
else
    print_warning "Limited workspace dependency usage ($WORKSPACE_DEPS found)"
fi
echo ""

# 8. Code Formatting
echo "8. Checking Code Formatting..."
if cargo fmt --all -- --check > /dev/null 2>&1; then
    print_success "All code properly formatted"
else
    print_failure "Code formatting issues found"
    echo "  Run: cargo fmt --all"
fi
echo ""

# 9. Test Success
echo "9. Checking Test Success..."
if command -v cargo-nextest &> /dev/null; then
    TEST_RESULT=$(cargo nextest run --all-features 2>&1 | grep "Summary" | tail -100)
    if echo "$TEST_RESULT" | grep "0 failed" > /dev/null; then
        print_success "All tests passing"
        echo "  $TEST_RESULT"
    else
        print_failure "Tests failing"
        echo "  $TEST_RESULT"
    fi
else
    print_warning "cargo-nextest not installed, using cargo test"
    if cargo test --all-features > /dev/null 2>&1; then
        print_success "All tests passing"
    else
        print_failure "Tests failing"
    fi
fi
echo ""

# 10. File Statistics
echo "10. Project Statistics (via tokei)..."
if command -v tokei &> /dev/null; then
    tokei --sort code | head -100
else
    print_warning "tokei not installed (install with: cargo install tokei)"
fi
echo ""

# Summary
echo "======================================================================"
if [ "$OVERALL_SUCCESS" = true ]; then
    echo -e "${GREEN}All Policy Checks Passed!${NC}"
    exit 0
else
    echo -e "${RED}Some Policy Checks Failed - Please Review Above${NC}"
    exit 1
fi
