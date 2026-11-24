use serde::{Deserialize, Serialize};

/// Available UI color themes for XolHN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeId {
    #[default]
    XolhnDark,
    XolhnLight,
    RosePine,
    CatppuccinMocha,
    CatppuccinLatte,
    GruvboxDark,
}

impl ThemeId {
    /// All supported themes in display order.
    pub const ALL: [ThemeId; 6] = [
        Self::XolhnDark,
        Self::XolhnLight,
        Self::RosePine,
        Self::CatppuccinMocha,
        Self::CatppuccinLatte,
        Self::GruvboxDark,
    ];

    /// Human-readable theme name for UI labels.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::XolhnDark => "XolHN Dark",
            Self::XolhnLight => "XolHN Light",
            Self::RosePine => "Rosé Pine",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::GruvboxDark => "Gruvbox Dark",
        }
    }

    /// Kebab-case identifier matching CSS/serialized theme slugs.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::XolhnDark => "xolhn-dark",
            Self::XolhnLight => "xolhn-light",
            Self::RosePine => "rose-pine",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::GruvboxDark => "gruvbox-dark",
        }
    }

    /// Resolves a theme from its kebab-case slug.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "xolhn-dark" => Some(Self::XolhnDark),
            "xolhn-light" => Some(Self::XolhnLight),
            "rose-pine" => Some(Self::RosePine),
            "catppuccin-mocha" => Some(Self::CatppuccinMocha),
            "catppuccin-latte" => Some(Self::CatppuccinLatte),
            "gruvbox-dark" => Some(Self::GruvboxDark),
            _ => None,
        }
    }

    /// Returns `true` if this theme is a dark theme.
    pub const fn is_dark(self) -> bool {
        match self {
            Self::XolhnDark | Self::RosePine | Self::CatppuccinMocha | Self::GruvboxDark => true,
            Self::XolhnLight | Self::CatppuccinLatte => false,
        }
    }

    /// Cycles to the next theme in [`ThemeId::ALL`].
    pub const fn next(self) -> Self {
        match self {
            Self::XolhnDark => Self::XolhnLight,
            Self::XolhnLight => Self::RosePine,
            Self::RosePine => Self::CatppuccinMocha,
            Self::CatppuccinMocha => Self::CatppuccinLatte,
            Self::CatppuccinLatte => Self::GruvboxDark,
            Self::GruvboxDark => Self::XolhnDark,
        }
    }

    /// Returns the `0xRRGGBB` color palette for this theme.
    pub const fn palette(self) -> ThemePalette {
        match self {
            Self::XolhnDark => ThemePalette {
                bg: 0x111216,
                surface: 0x181a20,
                surface_2: 0x21242d,
                hover: 0x1e212a,
                border: 0x2a2e3a,
                text: 0xeceef2,
                muted: 0x98a0b2,
                subtle: 0x687082,
                accent: 0xff6b1a,
                accent_fg: 0xffffff,
                link: 0x60a5fa,
                code_bg: 0x0c0d10,
                rails: [0xff6b1a, 0x60a5fa, 0x34d399, 0xc084fc],
                is_dark: true,
            },
            Self::XolhnLight => ThemePalette {
                bg: 0xf6f4f0,
                surface: 0xffffff,
                surface_2: 0xefece6,
                hover: 0xf3f0ea,
                border: 0xe2ddd5,
                text: 0x1c1917,
                muted: 0x57534e,
                subtle: 0x857f78,
                accent: 0xe05300,
                accent_fg: 0xffffff,
                link: 0x1d4ed8,
                code_bg: 0xedeae3,
                rails: [0xe05300, 0x2563eb, 0x059669, 0x7c3aed],
                is_dark: false,
            },
            Self::RosePine => ThemePalette {
                bg: 0x191724,
                surface: 0x1f1d2e,
                surface_2: 0x26233a,
                hover: 0x21202e,
                border: 0x403d52,
                text: 0xe0def4,
                muted: 0x908caa,
                subtle: 0x6e6a86,
                accent: 0xf6c177,
                accent_fg: 0x191724,
                link: 0x9ccfd8,
                code_bg: 0x151320,
                rails: [0xeb6f92, 0xf6c177, 0x9ccfd8, 0xc4a7e7],
                is_dark: true,
            },
            Self::CatppuccinMocha => ThemePalette {
                bg: 0x1e1e2e,
                surface: 0x181825,
                surface_2: 0x313244,
                hover: 0x242438,
                border: 0x45475a,
                text: 0xcdd6f4,
                muted: 0xa6adc8,
                subtle: 0x6c7086,
                accent: 0xfab387,
                accent_fg: 0x11111b,
                link: 0x89b4fa,
                code_bg: 0x11111b,
                rails: [0xfab387, 0x89b4fa, 0xa6e3a1, 0xcba6f7],
                is_dark: true,
            },
            Self::CatppuccinLatte => ThemePalette {
                bg: 0xeff1f5,
                surface: 0xe6e9ef,
                surface_2: 0xdce0e8,
                hover: 0xe2e5ec,
                border: 0xccd0da,
                text: 0x4c4f69,
                muted: 0x6c6f85,
                subtle: 0x9ca0b0,
                accent: 0xfe640b,
                accent_fg: 0xffffff,
                link: 0x1e66f5,
                code_bg: 0xe6e9ef,
                rails: [0xfe640b, 0x1e66f5, 0x40a02b, 0x8839ef],
                is_dark: false,
            },
            Self::GruvboxDark => ThemePalette {
                bg: 0x282828,
                surface: 0x1d2021,
                surface_2: 0x3c3836,
                hover: 0x32302f,
                border: 0x504945,
                text: 0xebdbb2,
                muted: 0xbdae93,
                subtle: 0x928374,
                accent: 0xfe8019,
                accent_fg: 0x1d2021,
                link: 0x83a598,
                code_bg: 0x1d2021,
                rails: [0xfe8019, 0x83a598, 0x8ec07c, 0xd3869b],
                is_dark: true,
            },
        }
    }
}

/// Complete `0xRRGGBB` color palette for a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub bg: u32,
    pub surface: u32,
    pub surface_2: u32,
    pub hover: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub subtle: u32,
    pub accent: u32,
    pub accent_fg: u32,
    pub link: u32,
    pub code_bg: u32,
    pub rails: [u32; 4],
    pub is_dark: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_have_distinct_palettes() {
        for (i, &lhs) in ThemeId::ALL.iter().enumerate() {
            for &rhs in &ThemeId::ALL[i + 1..] {
                assert_ne!(
                    lhs.palette(),
                    rhs.palette(),
                    "{lhs:?} and {rhs:?} should have distinct palettes"
                );
            }
        }
    }

    #[test]
    fn slug_round_trip() {
        for &theme in &ThemeId::ALL {
            assert_eq!(ThemeId::from_slug(theme.slug()), Some(theme));
        }
        assert_eq!(ThemeId::from_slug("nonexistent-theme"), None);
    }

    #[test]
    fn next_cycles_cleanly_over_all_themes() {
        let start = ThemeId::ALL[0];
        let mut current = start;
        let mut visited = Vec::with_capacity(ThemeId::ALL.len());

        for _ in 0..ThemeId::ALL.len() {
            visited.push(current);
            current = current.next();
        }

        assert_eq!(current, start);
        assert_eq!(visited.as_slice(), &ThemeId::ALL);
    }

    #[test]
    fn dark_and_light_flags_match_palettes() {
        let expected = [
            (ThemeId::XolhnDark, true),
            (ThemeId::XolhnLight, false),
            (ThemeId::RosePine, true),
            (ThemeId::CatppuccinMocha, true),
            (ThemeId::CatppuccinLatte, false),
            (ThemeId::GruvboxDark, true),
        ];

        for (theme, is_dark) in expected {
            assert_eq!(theme.is_dark(), is_dark);
            assert_eq!(theme.palette().is_dark, is_dark);
        }
    }

    #[test]
    fn serde_uses_kebab_case_slugs() {
        for &theme in &ThemeId::ALL {
            let serialized = serde_json::to_string(&theme).expect("serialize ThemeId");
            assert_eq!(serialized, format!("\"{}\"", theme.slug()));
            let deserialized: ThemeId =
                serde_json::from_str(&serialized).expect("deserialize ThemeId");
            assert_eq!(deserialized, theme);
        }
    }
}
