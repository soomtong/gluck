fn main() {
    let src_dir = std::path::Path::new("src");
    let mut c_config = cc::Build::new();
    // Generated/upstream C we never edit: drop cc's default warning level
    // (-Wall -Wextra / MSVC /W4) instead of probing per-warning -Wno flags,
    // which MSVC ignores.
    c_config.std("c11").include(src_dir).warnings(false);
    // scanner.c has non-ASCII comments; without this MSVC decodes it in the
    // system code page (e.g. CP949) and warns C4819.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        c_config.flag("-utf-8");
    }
    c_config.file(src_dir.join("parser.c"));
    c_config.file(src_dir.join("scanner.c"));
    c_config.compile("tree-sitter-lisette");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/scanner.c");
    println!("cargo:rerun-if-changed=src/tree_sitter");
}
