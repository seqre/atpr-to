# atpr.to — development and deployment tasks

# List available recipes
default:
    @just --list

# Build release binary for Lambda (arm64)
build:
    cargo lambda build --release --arm64

# Run tests
test:
    cargo test

# Regenerate src/generated/ from lexicons/ (checked into git — commit the result)
codegen:
    cargo run --features codegen --bin codegen

# Fail if src/generated/ is stale relative to lexicons/
codegen-check: codegen
    git diff --exit-code -- src/generated

# Audit dependencies and licences
deny:
    cargo deny check

# Run clippy
lint:
    cargo clippy --all-targets -- -D warnings

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Deploy to AWS via SAM (builds first)
deploy: build
    sam build
    sam deploy --guided

# Deploy without prompts (uses samconfig.toml from previous guided deploy)
deploy-fast: build
    sam build
    sam deploy

# Tail Lambda logs
logs:
    sam logs --name atpr-to --tail

# Run locally via Lambda runtime emulator
local:
    cargo lambda watch

# Generate test coverage report (requires cargo-llvm-cov)
#
# The gate is 80, and it is real. It was 100, which never passed: measured
# against the pre-overhaul tree the number is 83.17%, so `--fail-under-lines
# 100` exited 1. cargo-llvm-cov has no `coverage:excl` marker support -- only
# --ignore-filename-regex and #[coverage(off)] -- so the 25 markers that were
# supposed to make 100% attainable did nothing at all.
coverage:
    cargo llvm-cov --ignore-filename-regex 'src/generated|src/main|src/bin' --fail-under-lines 80 --html
