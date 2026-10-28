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
    Zig,
    Asm,
    LinkerScript,
    Lisette,
    Gleam,
    Ruby,
    Lua,
    Kotlin,
    Scala,
    Elixir,
    Erlang,
    Odin,
    Php,
    LlvmIr,
    Clojure,
    Janet,
    Perl,
}

impl Language {
    pub fn from_path(path: &str) -> Option<Self> {
        let path = Path::new(path);
        if let Some(lang) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(Self::from_file_name)
        {
            return Some(lang);
        }
        let ext = path.extension().and_then(|e| e.to_str())?;
        match ext {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "js" | "mjs" => Some(Self::JavaScript),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "go" => Some(Self::Go),
            "c" | "h" => Some(Self::C),
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ipp" | "tpp"
            | "cppm" | "ixx" => Some(Self::Cpp),
            "java" => Some(Self::Java),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "zig" => Some(Self::Zig),
            "s" | "S" | "asm" => Some(Self::Asm),
            "ld" | "lds" => Some(Self::LinkerScript),
            "lis" => Some(Self::Lisette),
            "gleam" => Some(Self::Gleam),
            "rb" | "rake" | "gemspec" | "ru" => Some(Self::Ruby),
            "lua" => Some(Self::Lua),
            "kt" | "kts" => Some(Self::Kotlin),
            "scala" | "sc" | "sbt" => Some(Self::Scala),
            "ex" | "exs" => Some(Self::Elixir),
            "erl" | "hrl" | "escript" => Some(Self::Erlang),
            "odin" => Some(Self::Odin),
            "php" | "phtml" => Some(Self::Php),
            "ll" => Some(Self::LlvmIr),
            "clj" | "cljs" | "cljc" | "edn" | "bb" => Some(Self::Clojure),
            "janet" | "jdn" => Some(Self::Janet),
            "pl" | "pm" => Some(Self::Perl),
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
            Self::Zig => "zig",
            Self::Asm => "asm",
            Self::LinkerScript => "linkerscript",
            Self::Lisette => "lisette",
            Self::Gleam => "gleam",
            Self::Ruby => "ruby",
            Self::Lua => "lua",
            Self::Kotlin => "kotlin",
            Self::Scala => "scala",
            Self::Elixir => "elixir",
            Self::Erlang => "erlang",
            Self::Odin => "odin",
            Self::Php => "php",
            Self::LlvmIr => "llvm",
            Self::Clojure => "clojure",
            Self::Janet => "janet",
            Self::Perl => "perl",
        }
    }

    /// Extension-less files whose name alone identifies the language.
    fn from_file_name(name: &str) -> Option<Self> {
        match name {
            "Rakefile" | "Gemfile" => Some(Self::Ruby),
            "rebar.config" => Some(Self::Erlang),
            _ => None,
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

    #[test]
    fn detects_zig_and_shells() {
        assert_eq!(Language::from_path("main.zig"), Some(Language::Zig));
        assert_eq!(Language::from_path("run.sh"), Some(Language::Bash));
        assert_eq!(Language::from_path("run.bash"), Some(Language::Bash));
        assert_eq!(Language::from_path("run.zsh"), Some(Language::Bash));
        assert!(!Language::Zig.supports_symbol_chunking());
        assert!(!Language::Bash.supports_symbol_chunking());
    }

    #[test]
    fn detects_asm() {
        assert_eq!(Language::from_path("start.s"), Some(Language::Asm));
        assert_eq!(Language::from_path("boot.S"), Some(Language::Asm));
        assert_eq!(Language::from_path("core.asm"), Some(Language::Asm));
        assert!(!Language::Asm.supports_symbol_chunking());
    }

    #[test]
    fn detects_linker_scripts() {
        assert_eq!(
            Language::from_path("kernel.ld"),
            Some(Language::LinkerScript)
        );
        assert_eq!(
            Language::from_path("stm32.lds"),
            Some(Language::LinkerScript)
        );
        assert!(!Language::LinkerScript.supports_symbol_chunking());
    }

    #[test]
    fn detects_lisette() {
        assert_eq!(Language::from_path("main.lis"), Some(Language::Lisette));
        assert_eq!(
            Language::from_path("prelude.d.lis"),
            Some(Language::Lisette)
        );
        assert!(!Language::Lisette.supports_symbol_chunking());
    }

    #[test]
    fn detects_gleam() {
        assert_eq!(Language::from_path("src/app.gleam"), Some(Language::Gleam));
        assert!(!Language::Gleam.supports_symbol_chunking());
    }

    #[test]
    fn detects_batch_one_languages() {
        assert_eq!(Language::from_path("stubs.pyi"), Some(Language::Python));
        assert_eq!(Language::from_path("vec.hh"), Some(Language::Cpp));
        assert_eq!(Language::from_path("mod.cppm"), Some(Language::Cpp));
        assert_eq!(Language::from_path("Main.java"), Some(Language::Java));
        assert_eq!(Language::from_path("app.rb"), Some(Language::Ruby));
        assert_eq!(Language::from_path("gluck.gemspec"), Some(Language::Ruby));
        assert_eq!(Language::from_path("Rakefile"), Some(Language::Ruby));
        assert_eq!(Language::from_path("sub/Gemfile"), Some(Language::Ruby));
        assert_eq!(Language::from_path("init.lua"), Some(Language::Lua));
        assert!(!Language::Ruby.supports_symbol_chunking());
        assert!(!Language::Lua.supports_symbol_chunking());
    }

    #[test]
    fn detects_batch_two_languages() {
        assert_eq!(Language::from_path("Main.kt"), Some(Language::Kotlin));
        assert_eq!(
            Language::from_path("build.gradle.kts"),
            Some(Language::Kotlin)
        );
        assert_eq!(Language::from_path("App.scala"), Some(Language::Scala));
        assert_eq!(Language::from_path("build.sbt"), Some(Language::Scala));
        assert_eq!(Language::from_path("lib/app.ex"), Some(Language::Elixir));
        assert_eq!(Language::from_path("mix.exs"), Some(Language::Elixir));
        assert_eq!(Language::from_path("src/app.erl"), Some(Language::Erlang));
        assert_eq!(Language::from_path("rebar.config"), Some(Language::Erlang));
        assert_eq!(Language::from_path("main.odin"), Some(Language::Odin));
        assert_eq!(Language::from_path("index.php"), Some(Language::Php));
        assert_eq!(Language::from_path("add.ll"), Some(Language::LlvmIr));
        assert_eq!(Language::from_path("script.pl"), Some(Language::Perl));
        assert_eq!(Language::from_path("lib/Foo/Bar.pm"), Some(Language::Perl));
        // `.t` is left out: too generic to claim for Perl tests.
        assert_eq!(Language::from_path("basic.t"), None);
    }

    #[test]
    fn detects_lisps() {
        assert_eq!(
            Language::from_path("src/app/core.clj"),
            Some(Language::Clojure)
        );
        assert_eq!(Language::from_path("ui.cljs"), Some(Language::Clojure));
        assert_eq!(Language::from_path("deps.edn"), Some(Language::Clojure));
        assert_eq!(Language::from_path("main.janet"), Some(Language::Janet));
        assert_eq!(Language::from_path("project.jdn"), Some(Language::Janet));
    }
}
