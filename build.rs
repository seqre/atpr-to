use jacquard_lexicon::codegen::CodeGenerator;
use jacquard_lexicon::corpus::LexiconCorpus;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=lexicons/");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    let output_dir = Path::new("src/generated");

    // Clean previous output
    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir).expect("failed to clean generated dir");
    }

    let corpus = LexiconCorpus::load_from_dir("lexicons/").expect("failed to load lexicons");

    let codegen = CodeGenerator::new(&corpus, "crate::generated");

    codegen
        .write_to_disk(output_dir)
        .expect("failed to generate code");

    // Rename lib.rs -> mod.rs (codegen produces lib.rs for root module)
    let lib_rs = output_dir.join("lib.rs");
    let mod_rs = output_dir.join("mod.rs");
    if lib_rs.exists() {
        // Also strip feature gates — we don't use cargo features for our own lexicons
        let content = std::fs::read_to_string(&lib_rs).expect("failed to read lib.rs");
        let content = content
            .lines()
            .filter(|line| !line.starts_with("#[cfg(feature"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&mod_rs, content).expect("failed to write mod.rs");
        std::fs::remove_file(&lib_rs).expect("failed to remove lib.rs");
    }

    // Fix builder_types use path: crate::builder_types -> crate::generated::builder_types
    fix_builder_paths(output_dir);
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
