# atpr.to Progress

## Step 1: Lexicon & Generated Types ✅

**Implemented:**
- `lexicons/to.atpr.link.json` — lexicon definition for shortened URL records
- `build.rs` — uses `jacquard-lexicon` (corpus + codegen) to generate Rust types from lexicons
  - Runs `cargo:rerun-if-changed=lexicons/` so regeneration only happens when schemas change
  - Post-processes output: renames `lib.rs` → `mod.rs`, strips feature gates, fixes `crate::builder_types` paths
- `src/generated/` — auto-generated types: `Link<'a>` struct with builder, serde, collection impl
- Updated `Cargo.toml` with `jacquard-lexicon` as build-dependency (codegen feature)
- Added `jacquard-derive`, `rustversion`, `chrono` dependencies
- Removed old exploratory `http_handler.rs`

**Decisions:**
- Used `jacquard-lexicon` directly (not a separate `jacquard-lexgen` crate) — the codegen lives in `jacquard-lexicon` behind the `codegen` feature
- Generated code uses `jacquard_common::types::string::Uri` and `Datetime` wrappers (not plain strings)
- `Uri` normalizes URLs (adds trailing slash to bare domains)
- Builder pattern requires `Uri::new()` (returns Result) and `chrono::DateTime` for `created_at`

**Tests:** 3 passing
- Serde roundtrip (JSON → Link → JSON → Link)
- Builder API (construct Link with builder)
- Collection NSID constant = "to.atpr.link"
