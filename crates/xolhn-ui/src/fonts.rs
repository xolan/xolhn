use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

const ATKINSON_HYPERLEGIBLE_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/AtkinsonHyperlegible-Regular.ttf");
const ATKINSON_HYPERLEGIBLE_BOLD: &[u8] =
    include_bytes!("../assets/fonts/AtkinsonHyperlegible-Bold.ttf");
const ATKINSON_HYPERLEGIBLE_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/AtkinsonHyperlegible-Italic.ttf");
const INTER: &[u8] = include_bytes!("../assets/fonts/Inter.ttf");
const LITERATA: &[u8] = include_bytes!("../assets/fonts/Literata.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono.ttf");

/// Bundled sans-serif and serif UI font families in preferred display order.
pub const BUNDLED_UI_FONTS: &[&str] = &["Inter", "Atkinson Hyperlegible", "Literata"];

/// Bundled monospace font families in preferred display order.
pub const BUNDLED_MONO_FONTS: &[&str] = &["JetBrains Mono"];

const MONO_FONT_KEYWORDS: &[&str] = &[
    "mono",
    "code",
    "console",
    "courier",
    "consolas",
    "menlo",
    "fira",
    "hack",
    "iosevka",
    "cascadia",
    "jetbrains",
    "source code",
    "inconsolata",
    "victor",
    "fantasque",
    "hermit",
    "termin",
    "monaco",
    "droid sans mono",
    "noto sans mono",
    "dejavu sans mono",
    "liberation mono",
    "ubuntu mono",
];

/// Registers all bundled SIL OFL font files with GPUI's text system.
pub fn register_bundled_fonts(cx: &gpui::App) {
    let fonts = vec![
        Cow::Borrowed(ATKINSON_HYPERLEGIBLE_REGULAR),
        Cow::Borrowed(ATKINSON_HYPERLEGIBLE_BOLD),
        Cow::Borrowed(ATKINSON_HYPERLEGIBLE_ITALIC),
        Cow::Borrowed(INTER),
        Cow::Borrowed(LITERATA),
        Cow::Borrowed(JETBRAINS_MONO),
    ];

    if let Err(err) = cx.text_system().add_fonts(fonts) {
        log::warn!("Failed to register bundled fonts: {err}");
    }
}

/// Returns available UI font families with bundled UI fonts listed first.
pub fn list_ui_fonts(cx: &gpui::App) -> Vec<String> {
    let system_fonts = cx.text_system().all_font_names();
    merge_font_lists(BUNDLED_UI_FONTS, &system_fonts, false)
}

/// Returns available monospace font families with bundled and detected mono fonts listed first.
pub fn list_mono_fonts(cx: &gpui::App) -> Vec<String> {
    let system_fonts = cx.text_system().all_font_names();
    merge_font_lists(BUNDLED_MONO_FONTS, &system_fonts, true)
}

/// Enumerates system font names once and builds both UI and monospace font lists.
pub fn list_ui_and_mono_fonts(cx: &gpui::App) -> (Arc<Vec<String>>, Arc<Vec<String>>) {
    let system_fonts = cx.text_system().all_font_names();
    let ui = Arc::new(merge_font_lists(BUNDLED_UI_FONTS, &system_fonts, false));
    let mono = Arc::new(merge_font_lists(BUNDLED_MONO_FONTS, &system_fonts, true));
    (ui, mono)
}

/// Merges bundled font names with discovered system font names.
///
/// Bundled fonts are always placed first in their declared order (resolved
/// case-insensitively against `system_fonts` by exact match, then prefix match,
/// falling back to the constant name). Remaining system fonts are deduplicated
/// case-insensitively and sorted alphabetically, with detected monospace fonts
/// prioritized immediately after bundled fonts when `prioritize_mono` is true.
pub fn merge_font_lists(
    bundled: &[&str],
    system_fonts: &[String],
    prioritize_mono: bool,
) -> Vec<String> {
    let mut result = Vec::with_capacity(bundled.len() + system_fonts.len());
    let mut seen = HashSet::new();

    for &bundled_name in bundled {
        let resolved = resolve_bundled_font(bundled_name, system_fonts);
        if seen.insert(resolved.to_lowercase()) {
            result.push(resolved);
        }
    }

    let mut remaining: Vec<String> = Vec::new();
    for font in system_fonts {
        let trimmed = font.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_lowercase()) {
            remaining.push(trimmed.to_string());
        }
    }

    remaining.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });

    if prioritize_mono {
        let (mono, other): (Vec<String>, Vec<String>) = remaining
            .into_iter()
            .partition(|name| is_monospace_font(name));
        result.extend(mono);
        result.extend(other);
    } else {
        result.extend(remaining);
    }

    result
}

fn resolve_bundled_font(target: &str, system_fonts: &[String]) -> String {
    let target_lower = target.to_lowercase();

    if let Some(exact) = system_fonts
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty() && s.to_lowercase() == target_lower)
    {
        return exact.to_string();
    }

    if let Some(prefix) = system_fonts
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty() && s.to_lowercase().starts_with(&target_lower))
    {
        return prefix.to_string();
    }

    target.to_string()
}

fn is_monospace_font(name: &str) -> bool {
    let lower = name.to_lowercase();
    MONO_FONT_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_come_first_and_fall_back_when_absent() {
        let system = vec!["Times New Roman".to_string(), "Arial".to_string()];
        let merged = merge_font_lists(BUNDLED_UI_FONTS, &system, false);

        assert_eq!(
            merged,
            vec![
                "Inter",
                "Atkinson Hyperlegible",
                "Literata",
                "Arial",
                "Times New Roman",
            ]
        );
    }

    #[test]
    fn exact_case_insensitive_and_prefix_matching_against_system_fonts() {
        let system = vec![
            "inter".to_string(),
            "Atkinson Hyperlegible Next".to_string(),
            "LITERATA".to_string(),
            "Roboto".to_string(),
        ];
        let merged = merge_font_lists(BUNDLED_UI_FONTS, &system, false);

        assert_eq!(
            merged,
            vec!["inter", "Atkinson Hyperlegible Next", "LITERATA", "Roboto",]
        );
    }

    #[test]
    fn exact_match_takes_precedence_over_prefix_match() {
        let system = vec![
            "Inter Tight".to_string(),
            "Inter".to_string(),
            "Atkinson Hyperlegible".to_string(),
            "Literata".to_string(),
        ];
        let merged = merge_font_lists(BUNDLED_UI_FONTS, &system, false);

        assert_eq!(
            merged,
            vec!["Inter", "Atkinson Hyperlegible", "Literata", "Inter Tight",]
        );
    }

    #[test]
    fn deduplicates_case_insensitively_and_skips_blank_names() {
        let system = vec![
            "  ".to_string(),
            "Roboto".to_string(),
            "roboto".to_string(),
            "Arial".to_string(),
            "INTER".to_string(),
            "Inter".to_string(),
        ];
        let merged = merge_font_lists(BUNDLED_UI_FONTS, &system, false);

        assert_eq!(
            merged,
            vec![
                "INTER",
                "Atkinson Hyperlegible",
                "Literata",
                "Arial",
                "Roboto",
            ]
        );
    }

    #[test]
    fn mono_prioritization_places_detected_mono_fonts_after_bundled_mono() {
        let system = vec![
            "Times New Roman".to_string(),
            "Fira Code".to_string(),
            "Arial".to_string(),
            "jetbrains mono".to_string(),
            "Iosevka".to_string(),
            "Cascadia Code".to_string(),
            "DejaVu Sans".to_string(),
        ];
        let merged = merge_font_lists(BUNDLED_MONO_FONTS, &system, true);

        assert_eq!(
            merged,
            vec![
                "jetbrains mono",
                "Cascadia Code",
                "Fira Code",
                "Iosevka",
                "Arial",
                "DejaVu Sans",
                "Times New Roman",
            ]
        );
    }

    #[test]
    fn mono_keyword_detection_covers_expected_families() {
        for family in [
            "JetBrains Mono",
            "FiraCode Nerd Font",
            "Source Code Pro",
            "Inconsolata",
            "Hack",
            "Menlo",
            "Monaco",
            "Consolas",
            "Courier New",
            "Iosevka Term",
            "VictorMono",
            "FantasqueSansM",
            "Hermit",
            "Terminus",
            "Ubuntu Mono",
            "Liberation Mono",
            "DejaVu Sans Mono",
            "Noto Sans Mono",
            "Droid Sans Mono",
            "Lucida Console",
        ] {
            assert!(
                is_monospace_font(family),
                "expected mono match for {family}"
            );
        }

        for family in ["Inter", "Roboto", "Times New Roman", "Georgia"] {
            assert!(
                !is_monospace_font(family),
                "did not expect mono match for {family}"
            );
        }
    }
}
