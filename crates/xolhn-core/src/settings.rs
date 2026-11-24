use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::model::CommentSort;
use crate::theme::ThemeId;

/// Minimum allowed base UI font size in pixels.
pub const MIN_FONT_SIZE_PX: f32 = 12.0;
/// Maximum allowed base UI font size in pixels.
pub const MAX_FONT_SIZE_PX: f32 = 22.0;
/// Default base UI font size in pixels.
pub const DEFAULT_FONT_SIZE_PX: f32 = 15.0;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Line-height density presets for story and comment reading views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineHeightDensity {
    Compact,
    #[default]
    Comfortable,
    Relaxed,
}

impl LineHeightDensity {
    /// All available line-height density presets in display order.
    pub const ALL: [LineHeightDensity; 3] = [Self::Compact, Self::Comfortable, Self::Relaxed];

    /// Line-height multiplier relative to the base font size.
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::Compact => 1.35,
            Self::Comfortable => 1.52,
            Self::Relaxed => 1.72,
        }
    }

    /// Human-readable label for UI display.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Comfortable => "Comfortable",
            Self::Relaxed => "Relaxed",
        }
    }
}

/// Persistent user preferences for XolHN.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme: ThemeId,
    pub ui_font_family: Option<String>,
    pub mono_font_family: Option<String>,
    pub base_font_size_px: f32,
    pub line_height_density: LineHeightDensity,
    pub comment_sort: CommentSort,
    pub smooth_scrolling: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeId::XolhnDark,
            ui_font_family: None,
            mono_font_family: None,
            base_font_size_px: DEFAULT_FONT_SIZE_PX,
            line_height_density: LineHeightDensity::Comfortable,
            comment_sort: CommentSort::HnDefault,
            smooth_scrolling: true,
        }
    }
}

impl AppSettings {
    /// Clamps numeric preferences to valid bounds and normalizes empty font strings to `None`.
    pub fn clamped(mut self) -> Self {
        self.base_font_size_px = if self.base_font_size_px.is_finite() {
            self.base_font_size_px
                .clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX)
        } else {
            DEFAULT_FONT_SIZE_PX
        };
        self.ui_font_family = normalize_font_family(self.ui_font_family);
        self.mono_font_family = normalize_font_family(self.mono_font_family);
        self
    }
}

fn normalize_font_family(font: Option<String>) -> Option<String> {
    font.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// JSON file store for [`AppSettings`].
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: Option<PathBuf>,
}

impl SettingsStore {
    /// Creates a store backed by `settings.json` inside `config_dir`, or a no-op store if `None`.
    pub fn new(config_dir: Option<PathBuf>) -> Self {
        Self {
            path: config_dir.map(|dir| dir.join("settings.json")),
        }
    }

    /// Loads persisted settings, falling back to clamped defaults if missing or unreadable.
    pub fn load(&self) -> AppSettings {
        let Some(path) = &self.path else {
            return AppSettings::default().clamped();
        };

        let raw = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                log::warn!(
                    "Settings file not found at {}; using defaults",
                    path.display()
                );
                return AppSettings::default().clamped();
            }
            Err(err) => {
                log::warn!(
                    "Failed to read settings from {}: {err}; using defaults",
                    path.display()
                );
                return AppSettings::default().clamped();
            }
        };

        match serde_json::from_str::<AppSettings>(&raw) {
            Ok(settings) => settings.clamped(),
            Err(err) => {
                log::warn!(
                    "Failed to parse settings from {}: {err}; using defaults",
                    path.display()
                );
                AppSettings::default().clamped()
            }
        }
    }

    /// Atomically persists settings as pretty-printed JSON.
    pub fn save(&self, settings: &AppSettings) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        let parent = path
            .parent()
            .ok_or_else(|| format!("Settings path {} has no parent directory", path.display()))?;
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create settings directory {}: {err}",
                parent.display()
            )
        })?;

        let normalized = settings.clone().clamped();
        let json = serde_json::to_string_pretty(&normalized)
            .map_err(|err| format!("Failed to serialize settings: {err}"))?;

        let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_name = format!("settings.json.tmp.{}.{seq}", std::process::id());
        let tmp_path = parent.join(tmp_name);

        if let Err(err) = fs::write(&tmp_path, json) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!(
                "Failed to write temporary settings file {}: {err}",
                tmp_path.display()
            ));
        }

        if let Err(err) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!(
                "Failed to move settings file {} into place at {}: {err}",
                tmp_path.display(),
                path.display()
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let seq = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{prefix}_{}_{seq}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp test dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn defaults_match_expected_values() {
        let defaults = AppSettings::default();
        assert_eq!(defaults.theme, ThemeId::XolhnDark);
        assert_eq!(defaults.ui_font_family, None);
        assert_eq!(defaults.mono_font_family, None);
        assert_eq!(defaults.base_font_size_px, DEFAULT_FONT_SIZE_PX);
        assert_eq!(defaults.line_height_density, LineHeightDensity::Comfortable);
        assert_eq!(defaults.comment_sort, CommentSort::HnDefault);
        assert!(defaults.smooth_scrolling);

        assert_eq!(LineHeightDensity::ALL.len(), 3);
        assert_eq!(LineHeightDensity::Compact.multiplier(), 1.35);
        assert_eq!(LineHeightDensity::Comfortable.multiplier(), 1.52);
        assert_eq!(LineHeightDensity::Relaxed.multiplier(), 1.72);
        assert_eq!(LineHeightDensity::Compact.label(), "Compact");
        assert_eq!(LineHeightDensity::Comfortable.label(), "Comfortable");
        assert_eq!(LineHeightDensity::Relaxed.label(), "Relaxed");
    }

    #[test]
    fn clamped_handles_bounds_non_finite_and_blank_fonts() {
        let low = AppSettings {
            base_font_size_px: 5.0,
            ui_font_family: Some("   ".to_string()),
            mono_font_family: Some(String::new()),
            ..AppSettings::default()
        }
        .clamped();
        assert_eq!(low.base_font_size_px, MIN_FONT_SIZE_PX);
        assert_eq!(low.ui_font_family, None);
        assert_eq!(low.mono_font_family, None);

        let high = AppSettings {
            base_font_size_px: 99.0,
            ui_font_family: Some(" Inter ".to_string()),
            ..AppSettings::default()
        }
        .clamped();
        assert_eq!(high.base_font_size_px, MAX_FONT_SIZE_PX);
        assert_eq!(high.ui_font_family.as_deref(), Some("Inter"));

        let nan = AppSettings {
            base_font_size_px: f32::NAN,
            ..AppSettings::default()
        }
        .clamped();
        assert_eq!(nan.base_font_size_px, DEFAULT_FONT_SIZE_PX);

        let inf = AppSettings {
            base_font_size_px: f32::INFINITY,
            ..AppSettings::default()
        }
        .clamped();
        assert_eq!(inf.base_font_size_px, DEFAULT_FONT_SIZE_PX);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = TempDirGuard::new("xolhn_settings_roundtrip");
        let store = SettingsStore::new(Some(dir.path.clone()));

        let custom = AppSettings {
            theme: ThemeId::CatppuccinMocha,
            ui_font_family: Some("IBM Plex Sans".to_string()),
            mono_font_family: Some("JetBrains Mono".to_string()),
            base_font_size_px: 18.0,
            line_height_density: LineHeightDensity::Relaxed,
            comment_sort: CommentSort::Newest,
            smooth_scrolling: false,
        };

        store.save(&custom).expect("save settings");
        let loaded = store.load();
        assert_eq!(loaded, custom);
    }

    #[test]
    fn corrupt_json_and_missing_file_fall_back_to_defaults() {
        let dir = TempDirGuard::new("xolhn_settings_fallback");
        let store = SettingsStore::new(Some(dir.path.clone()));

        // Missing file returns defaults.
        assert_eq!(store.load(), AppSettings::default());

        // Corrupt JSON returns defaults.
        fs::write(dir.path.join("settings.json"), "{ not valid json").expect("write corrupt json");
        assert_eq!(store.load(), AppSettings::default());

        // Out-of-range JSON values are clamped on load.
        fs::write(
            dir.path.join("settings.json"),
            r#"{"base_font_size_px": 99.0, "ui_font_family": "  "}"#,
        )
        .expect("write out-of-range json");
        let clamped = store.load();
        assert_eq!(clamped.base_font_size_px, MAX_FONT_SIZE_PX);
        assert_eq!(clamped.ui_font_family, None);
        assert_eq!(clamped.theme, ThemeId::XolhnDark);
        assert!(clamped.smooth_scrolling);
    }

    #[test]
    fn none_config_dir_is_noop() {
        let store = SettingsStore::new(None);
        assert_eq!(store.load(), AppSettings::default());
        store
            .save(&AppSettings::default())
            .expect("save without config dir succeeds as no-op");
    }
}
