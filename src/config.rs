//! Application configuration and filesystem locations.
//!
//! Config lives in `~/.config/7player/config.toml`, cookies in
//! `~/.config/7player/cookies.json`, and downloaded audio in
//! `~/.cache/7player/audio`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::keymap::BindingValue;

pub const APP_DIR: &str = "7player";
pub const LEGACY_APP_DIR: &str = "ncm-tui";
pub const DEFAULT_THEME: &str = "tokyo-night";
pub const DEFAULT_QUALITY: &str = "exhigh";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Name of a built-in theme (see [`crate::ui::theme::THEME_NAMES`]).
    pub theme: String,
    /// Playback volume, 0-100.
    pub volume: u8,
    /// NetEase quality level: standard / higher / exhigh / lossless.
    pub quality: String,
    /// Whether the left navigation sidebar is visible.
    pub sidebar: bool,
    /// Per-action key overrides (`[keys]`). Empty means built-in defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, BindingValue>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_string(),
            volume: 70,
            quality: DEFAULT_QUALITY.to_string(),
            sidebar: true,
            keys: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Self>(&text) {
                Ok(mut cfg) => {
                    cfg.volume = cfg.volume.min(100);
                    if cfg.theme.trim().is_empty() {
                        cfg.theme = DEFAULT_THEME.to_string();
                    }
                    if cfg.quality.trim().is_empty() {
                        cfg.quality = DEFAULT_QUALITY.to_string();
                    }
                    cfg
                }
                Err(err) => {
                    eprintln!("配置文件解析失败（将使用默认值）: {err}");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建配置目录失败: {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("序列化配置失败")?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("写入配置失败: {}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("保存配置失败: {}", path.display()))?;
        Ok(())
    }

    pub fn path() -> PathBuf {
        config_dir().join("config.toml")
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR)
}

pub fn cookie_path() -> PathBuf {
    config_dir().join("cookies.json")
}

pub fn audio_cache_dir() -> PathBuf {
    cache_dir().join("audio")
}

pub fn log_path() -> PathBuf {
    cache_dir().join("7player.log")
}

/// Move the previous `ncm-tui` config/cache aside on first launch after the
/// rename, so existing login cookies, theme and audio cache are preserved.
pub fn migrate_legacy() {
    migrate_dir(dirs::config_dir(), "config");
    migrate_dir(dirs::cache_dir(), "cache");
}

fn migrate_dir(base: Option<PathBuf>, label: &str) {
    let Some(base) = base else { return };
    let new = base.join(APP_DIR);
    let old = base.join(LEGACY_APP_DIR);
    if new.exists() || !old.exists() {
        return;
    }
    if std::fs::rename(&old, &new).is_ok() {
        eprintln!(
            "已迁移旧 {label} 目录: {} -> {}",
            old.display(),
            new.display()
        );
    }
}

/// Validate/normalise a quality string. Returns `None` for unknown values.
pub fn normalize_quality(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "standard" | "std" | "128" => Some("standard"),
        "higher" | "192" => Some("higher"),
        "exhigh" | "320" => Some("exhigh"),
        "lossless" | "flac" => Some("lossless"),
        _ => None,
    }
}

pub fn quality_label(value: &str) -> &'static str {
    match normalize_quality(value).unwrap_or(DEFAULT_QUALITY) {
        "standard" => "标准 128k",
        "higher" => "较高 192k",
        "lossless" => "无损 FLAC",
        _ => "极高 320k",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_quality_is_known() {
        assert_eq!(normalize_quality(DEFAULT_QUALITY), Some("exhigh"));
        assert_eq!(normalize_quality("FLAC"), Some("lossless"));
        assert_eq!(normalize_quality("nope"), None);
    }

    #[test]
    fn config_roundtrip() {
        let mut cfg = Config {
            theme: "dracula".into(),
            volume: 42,
            quality: "lossless".into(),
            sidebar: true,
            keys: BTreeMap::new(),
        };
        cfg.keys.insert(
            "move_up".to_string(),
            BindingValue::Many(vec!["k".into(), "up".into()]),
        );
        let text = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.theme, "dracula");
        assert_eq!(back.volume, 42);
        assert_eq!(back.quality, "lossless");
        assert_eq!(back.keys.len(), 1);
    }
}
