fn main() {
    let src_dir = std::path::Path::new("src");
    let mut c_config = cc::Build::new();
    // Generated upstream C we never edit: drop cc's default warning level
    // (-Wall -Wextra / MSVC /W4) instead of probing per-warning -Wno flags,
    // which MSVC ignores.
    c_config.std("c11").include(src_dir).warnings(false);
    c_config.file(src_dir.join("parser.c"));
    c_config.compile("tree-sitter-linkerscript");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/tree_sitter");
}
