pub mod fonts;
pub mod pull_to_refresh;
pub mod smooth_scroll;
pub mod state;
pub mod theme_bridge;
pub mod views;

pub use state::{AppPaths, ChromeCallback, SafeAreaInsets, SafeAreaProvider};
pub use views::app_view::{XolhnApp, bind_keys};

use gpui::AppContext as _;
use xolhn_core::settings::SettingsStore;

#[derive(Clone, Default)]
struct XolhnRuntimeConfig {
    paths: AppPaths,
    chrome_cb: Option<ChromeCallback>,
}

impl gpui::Global for XolhnRuntimeConfig {}

#[derive(Clone)]
struct SafeAreaProviderGlobal(SafeAreaProvider);

impl gpui::Global for SafeAreaProviderGlobal {}

/// Registers a platform provider for logical-pixel safe area insets.
pub fn set_safe_area_provider(cx: &mut gpui::App, provider: SafeAreaProvider) {
    cx.set_global(SafeAreaProviderGlobal(provider));
}

pub(crate) fn effective_safe_area(window: &gpui::Window, cx: &gpui::App) -> SafeAreaInsets {
    let win_insets = window.insets().effective();
    let from_window = SafeAreaInsets {
        top: f32::from(win_insets.top),
        bottom: f32::from(win_insets.bottom),
        left: f32::from(win_insets.left),
        right: f32::from(win_insets.right),
    };
    let from_provider = cx
        .try_global::<SafeAreaProviderGlobal>()
        .map(|provider| (provider.0)(window.scale_factor()))
        .unwrap_or_default();
    from_window.max_edges(from_provider)
}

/// Initializes GPUI Kit, registers bundled fonts, applies persisted theme & typography,
/// and installs keyboard bindings for XolHN.
pub fn init_app(cx: &mut gpui::App, paths: AppPaths, chrome_cb: Option<ChromeCallback>) {
    gpui_kit::init(cx);
    fonts::register_bundled_fonts(cx);

    let settings = SettingsStore::new(paths.config_dir.clone()).load();
    theme_bridge::apply_theme_and_typography(&settings, None, cx);
    if let Some(cb) = &chrome_cb {
        cb(settings.theme.palette());
    }

    bind_keys(cx);
    cx.set_global(XolhnRuntimeConfig { paths, chrome_cb });
}

/// Builds the root `XolhnApp` entity inside a newly opened window.
pub fn create_root_view(window: &mut gpui::Window, cx: &mut gpui::App) -> gpui::Entity<XolhnApp> {
    let config = cx
        .try_global::<XolhnRuntimeConfig>()
        .cloned()
        .unwrap_or_default();
    cx.new(|cx| XolhnApp::new(config.paths, config.chrome_cb, window, cx))
}
