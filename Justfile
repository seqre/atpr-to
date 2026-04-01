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
coverage:
    cargo llvm-cov --ignore-filename-regex 'src/generated|src/main' --fail-under-lines 100 --html

# Check for semver-incompatible API changes (requires cargo-semver-checks)
semver:
    cargo semver-checks
