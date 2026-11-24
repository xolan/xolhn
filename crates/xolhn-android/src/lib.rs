#[cfg(target_os = "android")]
extern crate gpui_mobile;

#[cfg(target_os = "android")]
use gpui::AppContext as _;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("xolhn"),
    );

    gpui_mobile::android::jni::install_panic_hook();

    let _platform = gpui_mobile::android::jni::init_platform(&app);

    let Some(shared) = gpui_mobile::android::jni::shared_platform() else {
        log::error!("shared_platform() returned None");
        return;
    };

    let base = app.internal_data_path();
    let paths = xolhn_ui::AppPaths {
        config_dir: base.as_ref().map(|d| d.join("config")),
        cache_dir: base.as_ref().map(|d| d.join("cache")),
    };

    let chrome_cb: xolhn_ui::ChromeCallback = std::sync::Arc::new(|palette| {
        let style = gpui_mobile::SystemChromeStyle {
            status_bar_color: Some(palette.surface),
            status_bar_style: if palette.is_dark {
                gpui_mobile::StatusBarContentStyle::Light
            } else {
                gpui_mobile::StatusBarContentStyle::Dark
            },
            navigation_bar_color: Some(palette.surface),
        };
        gpui_mobile::set_system_chrome(&style);
    });

    gpui::Application::with_platform(shared.into_rc())
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut gpui::App| {
            xolhn_ui::init_app(cx, paths, Some(chrome_cb));
            gpui_kit::component::input::set_native_touch_selection(true, cx);

            if let Err(err) = cx.open_window(
                gpui::WindowOptions {
                    window_bounds: None,
                    ..Default::default()
                },
                |window, cx| {
                    let view = xolhn_ui::create_root_view(window, cx);
                    cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
                },
            ) {
                log::error!("Failed to open XolHN Android window: {err:#}");
            }
            cx.activate(true);
        });
}

#[cfg(not(target_os = "android"))]
pub fn android_platform_only() {}
