use gpui::{App, Hsla, Window, px, rgb};
use gpui_kit::component::{Theme, ThemeMode};
use xolhn_core::settings::AppSettings;
use xolhn_core::theme::{ThemeId, ThemePalette};

/// Converts a `0xRRGGBB` hex color into an opaque [`gpui::Hsla`].
pub fn rgb_hex(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Converts a `0xRRGGBB` hex color into a [`gpui::Hsla`] with an explicit alpha channel in `0.0..=1.0`.
pub fn rgba_hex(hex: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(hex).into();
    color.a = alpha.clamp(0.0, 1.0);
    color
}

/// Resolved GPUI [`Hsla`] palette for a XolHN theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiColors {
    pub bg: Hsla,
    pub surface: Hsla,
    pub surface_2: Hsla,
    pub hover: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub subtle: Hsla,
    pub accent: Hsla,
    pub accent_fg: Hsla,
    pub accent_soft: Hsla,
    pub link: Hsla,
    pub code_bg: Hsla,
    pub rails: [Hsla; 4],
    pub is_dark: bool,
}

impl From<ThemePalette> for UiColors {
    fn from(palette: ThemePalette) -> Self {
        let soft_alpha = if palette.is_dark { 0.15 } else { 0.12 };
        Self {
            bg: rgb_hex(palette.bg),
            surface: rgb_hex(palette.surface),
            surface_2: rgb_hex(palette.surface_2),
            hover: rgb_hex(palette.hover),
            border: rgb_hex(palette.border),
            text: rgb_hex(palette.text),
            muted: rgb_hex(palette.muted),
            subtle: rgb_hex(palette.subtle),
            accent: rgb_hex(palette.accent),
            accent_fg: rgb_hex(palette.accent_fg),
            accent_soft: rgba_hex(palette.accent, soft_alpha),
            link: rgb_hex(palette.link),
            code_bg: rgb_hex(palette.code_bg),
            rails: palette.rails.map(rgb_hex),
            is_dark: palette.is_dark,
        }
    }
}

impl From<ThemeId> for UiColors {
    fn from(theme_id: ThemeId) -> Self {
        Self::from(theme_id.palette())
    }
}

/// Synchronizes [`AppSettings`] theme colors and typography into the global [`gpui_kit::component::Theme`]
/// and optional active [`Window`].
pub fn apply_theme_and_typography(
    settings: &AppSettings,
    window: Option<&mut Window>,
    cx: &mut App,
) {
    let clamped = settings.clone().clamped();
    let palette = clamped.theme.palette();
    let ui = UiColors::from(palette);
    let target_mode = if palette.is_dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };

    // Load base light/dark defaults (including default platform font families when custom fonts are None)
    // before overriding palette colors in a subsequent `Theme::update`.
    Theme::change(target_mode, None, cx);

    Theme::update(cx, |theme| {
        theme.colors.background = ui.bg;
        theme.colors.foreground = ui.text;

        theme.colors.secondary = ui.surface_2;
        theme.colors.secondary_foreground = ui.text;
        theme.colors.secondary_hover = ui.hover;
        theme.colors.secondary_active = ui.surface_2;

        theme.colors.muted = ui.surface_2;
        theme.colors.muted_foreground = ui.muted;

        theme.colors.accent = ui.accent_soft;
        theme.colors.accent_foreground = ui.text;

        theme.colors.primary = ui.accent;
        theme.colors.primary_foreground = ui.accent_fg;
        theme.colors.primary_hover = rgba_hex(palette.accent, 0.9);
        theme.colors.primary_active = rgba_hex(palette.accent, 0.8);

        theme.colors.button = ui.surface_2;
        theme.colors.button_foreground = ui.text;
        theme.colors.button_hover = ui.hover;
        theme.colors.button_active = ui.surface;

        theme.colors.button_primary = ui.accent;
        theme.colors.button_primary_foreground = ui.accent_fg;
        theme.colors.button_primary_hover = rgba_hex(palette.accent, 0.9);
        theme.colors.button_primary_active = rgba_hex(palette.accent, 0.8);

        theme.colors.button_secondary = ui.surface_2;
        theme.colors.button_secondary_foreground = ui.text;
        theme.colors.button_secondary_hover = ui.hover;
        theme.colors.button_secondary_active = ui.surface;

        theme.colors.border = ui.border;
        theme.colors.input = ui.border;
        theme.colors.ring = ui.accent;

        theme.colors.popover = ui.surface;
        theme.colors.popover_foreground = ui.text;

        theme.colors.list = ui.bg;
        theme.colors.list_hover = ui.hover;
        theme.colors.list_active = ui.accent_soft;
        theme.colors.list_active_border = ui.accent;
        theme.colors.list_even = ui.surface;
        theme.colors.list_head = ui.surface;

        theme.colors.sidebar = ui.surface;
        theme.colors.sidebar_foreground = ui.text;
        theme.colors.sidebar_border = ui.border;
        theme.colors.sidebar_accent = ui.accent_soft;
        theme.colors.sidebar_accent_foreground = ui.text;
        theme.colors.sidebar_primary = ui.accent;
        theme.colors.sidebar_primary_foreground = ui.accent_fg;

        theme.colors.link = ui.link;
        theme.colors.link_hover = rgba_hex(palette.link, 0.85);
        theme.colors.link_active = rgba_hex(palette.link, 0.75);

        theme.colors.scrollbar = rgba_hex(palette.bg, 0.0);
        theme.colors.scrollbar_thumb = ui.border;
        theme.colors.scrollbar_thumb_hover = ui.subtle;

        theme.colors.title_bar = ui.surface;
        theme.colors.title_bar_border = ui.border;

        theme.colors.selection = rgba_hex(palette.accent, 0.25);
        theme.colors.caret = ui.accent;

        theme.colors.switch = ui.surface_2;
        theme.colors.switch_thumb = ui.text;

        theme.colors.slider_bar = ui.accent;
        theme.colors.slider_thumb = ui.accent_fg;

        theme.colors.tab = ui.surface;
        theme.colors.tab_active = ui.bg;
        theme.colors.tab_active_foreground = ui.text;
        theme.colors.tab_bar = ui.surface;
        theme.colors.tab_foreground = ui.muted;

        theme.colors.group_box = ui.surface;
        theme.colors.group_box_foreground = ui.text;

        theme.font_size = px(clamped.base_font_size_px);
        theme.mono_font_size = px((clamped.base_font_size_px - 1.5).max(11.0));

        if let Some(ui_font) = &clamped.ui_font_family {
            theme.font_family = ui_font.clone().into();
        }
        if let Some(mono_font) = &clamped.mono_font_family {
            theme.mono_font_family = mono_font.clone().into();
        }
    });

    if let Some(window) = window {
        window.set_rem_size(px(clamped.base_font_size_px));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xolhn_dark_converts_colors_and_sets_accent_soft_alpha() {
        let palette = ThemeId::XolhnDark.palette();
        let colors = UiColors::from(ThemeId::XolhnDark);

        assert!(colors.is_dark);
        assert_eq!(colors.bg, rgb_hex(palette.bg));
        assert_eq!(colors.surface, rgb_hex(palette.surface));
        assert_eq!(colors.surface_2, rgb_hex(palette.surface_2));
        assert_eq!(colors.hover, rgb_hex(palette.hover));
        assert_eq!(colors.border, rgb_hex(palette.border));
        assert_eq!(colors.text, rgb_hex(palette.text));
        assert_eq!(colors.muted, rgb_hex(palette.muted));
        assert_eq!(colors.subtle, rgb_hex(palette.subtle));
        assert_eq!(colors.accent, rgb_hex(palette.accent));
        assert_eq!(colors.accent_fg, rgb_hex(palette.accent_fg));
        assert_eq!(colors.link, rgb_hex(palette.link));
        assert_eq!(colors.code_bg, rgb_hex(palette.code_bg));
        assert_eq!(colors.rails, palette.rails.map(rgb_hex));

        assert!((colors.accent_soft.a - 0.15).abs() < f32::EPSILON);
        assert_eq!(colors.accent_soft.h, colors.accent.h);
        assert_eq!(colors.accent_soft.s, colors.accent.s);
        assert_eq!(colors.accent_soft.l, colors.accent.l);
    }

    #[test]
    fn xolhn_light_converts_colors_and_sets_accent_soft_alpha() {
        let palette = ThemeId::XolhnLight.palette();
        let colors = UiColors::from(ThemeId::XolhnLight);

        assert!(!colors.is_dark);
        assert_eq!(colors.bg, rgb_hex(palette.bg));
        assert_eq!(colors.surface, rgb_hex(palette.surface));
        assert_eq!(colors.surface_2, rgb_hex(palette.surface_2));
        assert_eq!(colors.hover, rgb_hex(palette.hover));
        assert_eq!(colors.border, rgb_hex(palette.border));
        assert_eq!(colors.text, rgb_hex(palette.text));
        assert_eq!(colors.muted, rgb_hex(palette.muted));
        assert_eq!(colors.subtle, rgb_hex(palette.subtle));
        assert_eq!(colors.accent, rgb_hex(palette.accent));
        assert_eq!(colors.accent_fg, rgb_hex(palette.accent_fg));
        assert_eq!(colors.link, rgb_hex(palette.link));
        assert_eq!(colors.code_bg, rgb_hex(palette.code_bg));
        assert_eq!(colors.rails, palette.rails.map(rgb_hex));

        assert!((colors.accent_soft.a - 0.12).abs() < f32::EPSILON);
        assert_eq!(colors.accent_soft.h, colors.accent.h);
        assert_eq!(colors.accent_soft.s, colors.accent.s);
        assert_eq!(colors.accent_soft.l, colors.accent.l);
    }

    #[test]
    fn rgba_hex_clamps_alpha_bounds() {
        let low = rgba_hex(0xff6b1a, -0.5);
        assert_eq!(low.a, 0.0);

        let high = rgba_hex(0xff6b1a, 1.5);
        assert_eq!(high.a, 1.0);
    }
}
