use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
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
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self)
            .context("failed to serialize config")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write config: {}", path.display()))?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("gluck");
    path.push("config.toml");
    path
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
        };
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.theme.name, "nord");
    }
}
