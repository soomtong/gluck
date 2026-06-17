use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    C,
    Cpp,
    Java,
    Bash,
    Toml,
    Json,
    Markdown,
    Html,
    Css,
    Swift,
    Yaml,
}

impl Language {
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path).extension().and_then(|e| e.to_str())?;
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "js" | "mjs" => Some(Self::JavaScript),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "go" => Some(Self::Go),
            "c" | "h" => Some(Self::C),
            "cpp" | "cc" | "cxx" | "hpp" => Some(Self::Cpp),
            "java" => Some(Self::Java),
            "sh" | "bash" => Some(Self::Bash),
            "toml" => Some(Self::Toml),
            "json" | "jsonc" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" => Some(Self::Markdown),
            "swift" => Some(Self::Swift),
            "html" => Some(Self::Html),
            "css" => Some(Self::Css),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Bash => "bash",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Css => "css",
            Self::Swift => "swift",
            Self::Yaml => "yaml",
        }
    }

    pub fn supports_symbol_chunking(&self) -> bool {
        matches!(
            self,
            Self::Rust
                | Self::Python
                | Self::JavaScript
                | Self::TypeScript
                | Self::Tsx
                | Self::Go
                | Self::Swift
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_extensions() {
        assert_eq!(Language::from_path("main.rs"), Some(Language::Rust));
        assert_eq!(Language::from_path("foo.py"), Some(Language::Python));
        assert_eq!(Language::from_path("foo.ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("foo.tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_path("foo.go"), Some(Language::Go));
        assert_eq!(Language::from_path("README"), None);
        assert_eq!(Language::from_path("foo.xyz"), None);
    }

    #[test]
    fn symbol_chunking_support_matrix() {
        assert!(Language::Rust.supports_symbol_chunking());
        assert!(Language::Tsx.supports_symbol_chunking());
        assert!(Language::Go.supports_symbol_chunking());
        assert!(!Language::Markdown.supports_symbol_chunking());
        assert!(!Language::C.supports_symbol_chunking());
        assert!(!Language::Java.supports_symbol_chunking());
    }

    #[test]
    fn detects_swift_jsonc_yaml() {
        assert_eq!(Language::from_path("main.swift"), Some(Language::Swift));
        assert_eq!(Language::from_path("config.yaml"), Some(Language::Yaml));
        assert_eq!(Language::from_path("config.yml"), Some(Language::Yaml));
        assert_eq!(Language::from_path("data.jsonc"), Some(Language::Json));
        assert!(Language::Swift.supports_symbol_chunking());
    }
}
