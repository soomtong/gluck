//! Endpoint resolution: `[ask]` config values win, empty ones fall back to
//! the preset of the same name and its API-key environment variable.

use crate::config::AskConfig;

/// An OpenAI-compatible chat completions endpoint with known defaults.
pub struct Preset {
    pub name: &'static str,
    pub env: &'static str,
    pub base_url: &'static str,
    pub model: &'static str,
}

/// Also the auto-detection order when `[ask].provider` is empty.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "openrouter",
        env: "OPENROUTER_API_KEY",
        base_url: "https://openrouter.ai/api/v1",
        model: "deepseek/deepseek-chat",
    },
    Preset {
        name: "deepseek",
        env: "DEEPSEEK_API_KEY",
        base_url: "https://api.deepseek.com/v1",
        model: "deepseek-chat",
    },
    Preset {
        name: "gemini",
        env: "GEMINI_API_KEY",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        model: "gemini-2.5-flash",
    },
    Preset {
        name: "groq",
        env: "GROQ_API_KEY",
        base_url: "https://api.groq.com/openai/v1",
        model: "llama-3.3-70b-versatile",
    },
    Preset {
        name: "cerebras",
        env: "CEREBRAS_API_KEY",
        base_url: "https://api.cerebras.ai/v1",
        model: "llama-3.3-70b",
    },
    Preset {
        name: "mistral",
        env: "MISTRAL_API_KEY",
        base_url: "https://api.mistral.ai/v1",
        model: "mistral-small-latest",
    },
    Preset {
        name: "openai",
        env: "OPENAI_API_KEY",
        base_url: "https://api.openai.com/v1",
        model: "gpt-4.1-mini",
    },
];

pub fn find_preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Config,
    Env,
    None,
}

#[derive(Clone, PartialEq)]
pub struct Endpoint {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub key_source: KeySource,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("key_source", &self.key_source)
            .finish()
    }
}

impl Endpoint {
    /// Short label for the panel title, e.g. `deepseek/deepseek-chat (env)`.
    pub fn label(&self) -> String {
        let source = match self.key_source {
            KeySource::Config => "config",
            KeySource::Env => "env",
            KeySource::None => "no key",
        };
        format!("{}/{} ({})", self.provider, self.model, source)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    #[error("no LLM provider configured")]
    NotConfigured,
    #[error("provider `{0}` has no base_url (not a known preset)")]
    MissingBaseUrl(String),
    #[error("provider `{0}` has no model (not a known preset)")]
    MissingModel(String),
    #[error("provider `{name}` has no API key (set api_key or {env})")]
    MissingKey { name: String, env: String },
}

pub fn resolve(cfg: &AskConfig) -> Result<Endpoint, ProviderError> {
    resolve_with(cfg, |k| std::env::var(k).ok())
}

/// `env` is injected so tests don't touch the process environment.
pub fn resolve_with(
    cfg: &AskConfig,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Endpoint, ProviderError> {
    let present = |k: &str| env(k).filter(|v| !v.trim().is_empty());

    let name = if cfg.provider.trim().is_empty() {
        PRESETS
            .iter()
            .find(|p| present(p.env).is_some())
            .map(|p| p.name.to_string())
            .ok_or(ProviderError::NotConfigured)?
    } else {
        cfg.provider.trim().to_string()
    };

    let entry = cfg.providers.get(&name).cloned().unwrap_or_default();
    let preset = find_preset(&name);
    let non_empty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());

    let base_url = non_empty(entry.base_url)
        .or_else(|| preset.map(|p| p.base_url.to_string()))
        .ok_or_else(|| ProviderError::MissingBaseUrl(name.clone()))?;
    let model = non_empty(entry.model)
        .or_else(|| preset.map(|p| p.model.to_string()))
        .ok_or_else(|| ProviderError::MissingModel(name.clone()))?;

    let key_env = non_empty(entry.api_key_env).or_else(|| preset.map(|p| p.env.to_string()));
    let (api_key, key_source) = match non_empty(entry.api_key) {
        Some(k) => (Some(k), KeySource::Config),
        None => match key_env.as_deref().and_then(present) {
            Some(k) => (Some(k), KeySource::Env),
            None => (None, KeySource::None),
        },
    };
    // Presets are hosted APIs that always need a key; a custom entry
    // without one is assumed to be a local server (e.g. Ollama).
    if api_key.is_none() && preset.is_some() {
        return Err(ProviderError::MissingKey {
            name,
            env: key_env.unwrap_or_default(),
        });
    }

    Ok(Endpoint {
        provider: name,
        base_url: base_url.trim_end_matches('/').to_string(),
        model,
        api_key,
        key_source,
    })
}

/// Help text shown in the Ask panel when no endpoint resolves.
pub fn setup_hint(err: &ProviderError) -> String {
    let envs: Vec<&str> = PRESETS.iter().map(|p| p.env).collect();
    format!(
        "{err}.\n\nAdd to {}:\n\n  [ask]\n  provider = \"deepseek\"\n\n  [ask.providers.deepseek]\n  api_key = \"sk-...\"\n  # model / base_url default to the preset\n\nOr leave `provider` empty and export one of: {}",
        crate::config::config_path().display(),
        envs.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn test_empty_provider_autodetects_first_preset_with_env() {
        let cfg = AskConfig::default();
        let ep = resolve_with(
            &cfg,
            env_of(&[("GROQ_API_KEY", "g"), ("DEEPSEEK_API_KEY", "d")]),
        )
        .unwrap();
        assert_eq!(ep.provider, "deepseek");
        assert_eq!(ep.model, "deepseek-chat");
        assert_eq!(ep.api_key.as_deref(), Some("d"));
        assert_eq!(ep.key_source, KeySource::Env);
    }

    #[test]
    fn test_nothing_configured_errors() {
        let cfg = AskConfig::default();
        assert_eq!(
            resolve_with(&cfg, env_of(&[])),
            Err(ProviderError::NotConfigured)
        );
    }

    #[test]
    fn test_config_values_win_over_env_and_preset() {
        let mut cfg = AskConfig {
            provider: "openrouter".into(),
            ..Default::default()
        };
        cfg.providers.insert(
            "openrouter".into(),
            ProviderConfig {
                model: Some("google/gemini-2.5-flash".into()),
                api_key: Some("from-config".into()),
                ..Default::default()
            },
        );
        let ep = resolve_with(&cfg, env_of(&[("OPENROUTER_API_KEY", "from-env")])).unwrap();
        assert_eq!(ep.api_key.as_deref(), Some("from-config"));
        assert_eq!(ep.key_source, KeySource::Config);
        assert_eq!(ep.model, "google/gemini-2.5-flash");
        assert_eq!(ep.base_url, "https://openrouter.ai/api/v1");
    }

    #[test]
    fn test_explicit_provider_falls_back_to_env_key() {
        let cfg = AskConfig {
            provider: "gemini".into(),
            ..Default::default()
        };
        let ep = resolve_with(&cfg, env_of(&[("GEMINI_API_KEY", "k")])).unwrap();
        assert_eq!(ep.key_source, KeySource::Env);
        assert_eq!(ep.model, "gemini-2.5-flash");
    }

    #[test]
    fn test_custom_api_key_env() {
        let mut cfg = AskConfig {
            provider: "deepseek".into(),
            ..Default::default()
        };
        cfg.providers.insert(
            "deepseek".into(),
            ProviderConfig {
                api_key_env: Some("MY_DS".into()),
                ..Default::default()
            },
        );
        let ep = resolve_with(&cfg, env_of(&[("MY_DS", "x")])).unwrap();
        assert_eq!(ep.api_key.as_deref(), Some("x"));
    }

    #[test]
    fn test_preset_without_any_key_errors() {
        let cfg = AskConfig {
            provider: "openai".into(),
            ..Default::default()
        };
        assert!(matches!(
            resolve_with(&cfg, env_of(&[])),
            Err(ProviderError::MissingKey { .. })
        ));
    }

    #[test]
    fn test_custom_local_provider_needs_no_key() {
        let mut cfg = AskConfig {
            provider: "local".into(),
            ..Default::default()
        };
        cfg.providers.insert(
            "local".into(),
            ProviderConfig {
                base_url: Some("http://localhost:11434/v1/".into()),
                model: Some("qwen".into()),
                ..Default::default()
            },
        );
        let ep = resolve_with(&cfg, env_of(&[])).unwrap();
        assert_eq!(ep.base_url, "http://localhost:11434/v1");
        assert_eq!(ep.api_key, None);
        assert_eq!(ep.key_source, KeySource::None);
    }

    #[test]
    fn test_unknown_provider_without_url_errors() {
        let cfg = AskConfig {
            provider: "nope".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve_with(&cfg, env_of(&[])),
            Err(ProviderError::MissingBaseUrl("nope".into()))
        );
    }

    #[test]
    fn test_endpoint_debug_masks_key() {
        let ep = Endpoint {
            provider: "x".into(),
            base_url: "u".into(),
            model: "m".into(),
            api_key: Some("sk-secret".into()),
            key_source: KeySource::Config,
        };
        assert!(!format!("{ep:?}").contains("sk-secret"));
    }
}
