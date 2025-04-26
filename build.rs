use std::path::PathBuf;

fn main() {
    let js_dir: PathBuf = ["tree-sitter-javascript", "src"].iter().collect();
    cc::Build::new()
        .include(&js_dir)
        .file(js_dir.join("parser.c"))
        .file(js_dir.join("scanner.c"))
        .compile("tree-sitter-javascript");
    println!("cargo:rerun-if-changed=tree-sitter-javascript/src/parser.c");
    println!("cargo:rerun-if-changed=tree-sitter-javascript/src/scanner.c");

    let ts_dir: PathBuf = ["tree-sitter-typescript", "typescript", "src"].iter().collect();
    cc::Build::new()
        .include(&ts_dir)
        .file(ts_dir.join("parser.c"))
        .file(ts_dir.join("scanner.c"))
        .compile("tree-sitter-typescript");
    println!("cargo:rerun-if-changed=tree-sitter-typescript/typescript/src/parser.c");
    println!("cargo:rerun-if-changed=tree-sitter-typescript/typescript/src/scanner.c");

    let tsx_dir: PathBuf = ["tree-sitter-typescript", "tsx", "src"].iter().collect();
    cc::Build::new()
        .include(&tsx_dir)
        .file(tsx_dir.join("parser.c"))
        .file(tsx_dir.join("scanner.c"))
        .compile("tree-sitter-tsx");
    println!("cargo:rerun-if-changed=tree-sitter-typescript/tsx/src/parser.c");
    println!("cargo:rerun-if-changed=tree-sitter-typescript/tsx/src/scanner.c");
} 