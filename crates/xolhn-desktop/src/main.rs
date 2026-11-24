use gpui_kit::{KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use xolhn_ui::AppPaths;

gpui_kit::actions!(xolhn_desktop, [Quit]);

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("xolhn_core=info,xolhn_ui=info,xolhn_desktop=info"),
    )
    .init();

    let paths = AppPaths {
        config_dir: dirs::config_dir().map(|d| d.join("xolhn")),
        cache_dir: dirs::cache_dir().map(|d| d.join("xolhn")),
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            xolhn_ui::init_app(cx, paths, None);

            cx.bind_keys([
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("ctrl-q", Quit, None),
            ]);
            cx.on_action(|_: &Quit, cx| cx.quit());

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
                window_min_size: Some(size(px(400.), px(560.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("XolHN".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };

            if let Err(err) = gpui_kit::open_window(options, cx, xolhn_ui::create_root_view) {
                log::error!("failed to open main window: {err:#}");
                cx.quit();
                return;
            }

            cx.activate(true);
        });
}
