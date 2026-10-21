use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub ask: AskConfig,
}

/// `[ask]`: LLM explain settings. Values set here win; anything left empty
/// falls back to the matching preset and its API-key environment variable
/// (see `crate::ask::provider`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AskConfig {
    /// Active `[ask.providers.<name>]` entry; empty = first preset whose
    /// fallback env var is set.
    pub provider: String,
    /// Language the answer is written in.
    pub language: String,
    pub detail: crate::ask::prompt::Detail,
    /// 0 = detail-based default.
    pub max_tokens: u32,
    /// Negative = omit the field (models that reject `temperature`).
    pub temperature: f32,
    /// Lines of surrounding code sent with a selection.
    pub extra_context_lines: usize,
    pub max_context_chars: usize,
    /// Replaces the built-in system prompt; `{language}` is substituted.
    pub system_prompt: String,
    pub providers: std::collections::BTreeMap<String, ProviderConfig>,
}

impl Default for AskConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            language: "Korean".to_string(),
            detail: crate::ask::prompt::Detail::Normal,
            max_tokens: 0,
            temperature: 0.2,
            extra_context_lines: 20,
            max_context_chars: 12_000,
            system_prompt: String::new(),
            providers: std::collections::BTreeMap::new(),
        }
    }
}

/// `[ask.providers.<name>]`. Every field is optional; unset ones come from
/// the preset of the same name.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Env var to read the key from when `api_key` is unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

// Hand-written so the key never ends up in logs or the debug overlay.
impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("api_key_env", &self.api_key_env)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub index_dir: Option<String>,
    pub batch_size: usize,
    pub max_file_bytes: usize,
    pub result_limit: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            index_dir: None,
            batch_size: 64,
            max_file_bytes: 1_000_000,
            result_limit: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: crate::theme::default_theme_name().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub scroll_lines: usize,
    /// Soft-wrap long lines in the View and Diff panes (toggled with `w`).
    pub word_wrap: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            scroll_lines: 3,
            word_wrap: true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        warn_if_readable_by_others(&path, &config);
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self).context("failed to serialize config")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write config: {}", path.display()))?;
        restrict_permissions(&path);
        Ok(())
    }
}

/// The config may hold API keys, so keep it owner-only.
#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &std::path::Path, config: &Config) {
    use std::os::unix::fs::PermissionsExt;
    let has_key = config.ask.providers.values().any(|p| p.api_key.is_some());
    let mode = std::fs::metadata(path).map(|m| m.permissions().mode());
    if has_key && mode.is_ok_and(|m| m & 0o077 != 0) {
        tracing::warn!(
            "{} holds API keys but is readable by other users; run `chmod 600` on it",
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &std::path::Path, _config: &Config) {}

pub fn config_path() -> PathBuf {
    let base = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("gluck").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_plain_theme() {
        let config = Config::default();
        assert_eq!(config.theme.name, "plain");
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config {
            theme: ThemeConfig {
                name: "nord".to_string(),
            },
            ui: UiConfig::default(),
            search: SearchConfig::default(),
            ask: AskConfig::default(),
        };
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.theme.name, "nord");
    }

    #[test]
    fn test_word_wrap_defaults_on_and_parses() {
        assert!(Config::default().ui.word_wrap);
        let config: Config = toml::from_str("[ui]\nword_wrap = false\n").unwrap();
        assert!(!config.ui.word_wrap);
        assert_eq!(config.ui.scroll_lines, 3);
    }

    #[test]
    fn test_ask_providers_parse_and_roundtrip_keep_key() {
        let src = "[ask]\nprovider = \"deepseek\"\n\n[ask.providers.deepseek]\napi_key = \"sk-test\"\n\n[ask.providers.local]\nbase_url = \"http://localhost:11434/v1\"\nmodel = \"qwen\"\n";
        let config: Config = toml::from_str(src).unwrap();
        assert_eq!(config.ask.provider, "deepseek");
        assert_eq!(config.ask.language, "Korean");
        let ds = &config.ask.providers["deepseek"];
        assert_eq!(ds.api_key.as_deref(), Some("sk-test"));
        assert!(ds.model.is_none());

        let back: Config = toml::from_str(&toml::to_string_pretty(&config).unwrap()).unwrap();
        assert_eq!(back.ask.providers, config.ask.providers);
        assert_eq!(back.ask.provider, "deepseek");
    }

    #[test]
    fn test_provider_debug_masks_key() {
        let p = ProviderConfig {
            api_key: Some("sk-secret".into()),
            ..Default::default()
        };
        let dbg = format!("{p:?}");
        assert!(!dbg.contains("sk-secret"));
        assert!(dbg.contains("***"));
    }
}
