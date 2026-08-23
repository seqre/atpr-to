//! Regenerates `src/generated/` from the Lexicon schemas in `lexicons/`.
//!
//! This used to be a `build.rs` that rewrote `src/` on every build, which broke
//! read-only and vendored checkouts, raced rust-analyzer, and forced CI to run
//! `cargo check` before `cargo fmt --check`. The generated code is now checked
//! into git and regenerated explicitly:
//!
//! ```sh
//! just codegen        # cargo run --features codegen --bin codegen
//! ```
//!
//! CI runs the same recipe and `git diff --exit-code`s the result, so a lexicon
//! change that is not accompanied by regenerated code fails the build.

use std::path::Path;

use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;

fn main() {
    let output_dir = Path::new("src/generated");

    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir).expect("failed to clean generated dir");
    }

    let corpus = LexiconCorpus::load_from_dir("lexicons/").expect("failed to load lexicons");
    let codegen = CodeGenerator::new(&corpus, "crate::generated");
    codegen
        .write_to_disk(output_dir)
        .expect("failed to generate code");

    // Codegen emits a crate root as `lib.rs`; this is a module, not a crate.
    let lib_rs = output_dir.join("lib.rs");
    let mod_rs = output_dir.join("mod.rs");
    if lib_rs.exists() {
        // Strip feature gates — we don't put our own lexicons behind cargo features.
        let content = std::fs::read_to_string(&lib_rs).expect("failed to read lib.rs");
        let content = content
            .lines()
            .filter(|line| !line.starts_with("#[cfg(feature"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&mod_rs, content).expect("failed to write mod.rs");
        std::fs::remove_file(&lib_rs).expect("failed to remove lib.rs");
    }

    // Codegen assumes it is generating a crate root, so builder types resolve as
    // `crate::builder_types`. Under `crate::generated` they are one level deeper.
    fix_builder_paths(output_dir);

    println!("regenerated {}", output_dir.display());
}

fn fix_builder_paths(dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("failed to read dir") {
        let entry = entry.expect("failed to read entry");
        let path = entry.path();
        if path.is_dir() {
            fix_builder_paths(&path);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let content = std::fs::read_to_string(&path).expect("failed to read file");
            if content.contains("crate::builder_types") {
                let fixed =
                    content.replace("crate::builder_types", "crate::generated::builder_types");
                std::fs::write(&path, fixed).expect("failed to write file");
            }
        }
    }
}
