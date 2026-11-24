#[cfg(target_os = "android")]
extern crate gpui_mobile;

#[cfg(target_os = "android")]
use gpui::AppContext as _;

#[cfg(target_os = "android")]
#[derive(Default)]
struct JavaSafeAreaState {
    received: bool,
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    generation: u64,
}

#[cfg(target_os = "android")]
static JAVA_SAFE_AREA_STATE: std::sync::LazyLock<(
    std::sync::Mutex<JavaSafeAreaState>,
    std::sync::Condvar,
)> = std::sync::LazyLock::new(|| {
    (
        std::sync::Mutex::new(JavaSafeAreaState::default()),
        std::sync::Condvar::new(),
    )
});

#[cfg(target_os = "android")]
struct SafeAreaWatcherStop(std::sync::Arc<std::sync::atomic::AtomicBool>);

#[cfg(target_os = "android")]
impl Drop for SafeAreaWatcherStop {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
        JAVA_SAFE_AREA_STATE.1.notify_all();
    }
}

#[cfg(target_os = "android")]
fn wait_for_safe_area_change(
    last_generation: u64,
    stop: &std::sync::atomic::AtomicBool,
) -> Option<u64> {
    let (lock, cvar) = &*JAVA_SAFE_AREA_STATE;
    let mut state = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    while state.generation == last_generation && !stop.load(std::sync::atomic::Ordering::Acquire) {
        state = match cvar.wait(state) {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
    }
    if stop.load(std::sync::atomic::Ordering::Acquire) {
        None
    } else {
        Some(state.generation)
    }
}

/// Receives system bar and display cutout safe-area insets in physical pixels from Java.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnSafeAreaInsets(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
) {
    let top = top.max(0);
    let bottom = bottom.max(0);
    let left = left.max(0);
    let right = right.max(0);

    let (lock, cvar) = &*JAVA_SAFE_AREA_STATE;
    let mut state = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let changed = !state.received
        || state.top != top
        || state.bottom != bottom
        || state.left != left
        || state.right != right;
    if !changed {
        return;
    }

    state.received = true;
    state.top = top;
    state.bottom = bottom;
    state.left = left;
    state.right = right;
    state.generation = state.generation.wrapping_add(1);
    drop(state);

    log::info!(
        "Android safe-area insets updated: top={} bottom={} left={} right={} (physical px)",
        top,
        bottom,
        left,
        right,
    );
    gpui_mobile::TEXT_INPUT_DIRTY.store(true, std::sync::atomic::Ordering::Release);
    cvar.notify_all();
}

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
            xolhn_ui::set_safe_area_provider(
                cx,
                std::sync::Arc::new(|scale_factor| {
                    let (lock, _) = &*JAVA_SAFE_AREA_STATE;
                    let state = match lock.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if state.received {
                        return xolhn_ui::SafeAreaInsets::from_physical_px(
                            state.top,
                            state.bottom,
                            state.left,
                            state.right,
                            scale_factor,
                        );
                    }
                    drop(state);

                    gpui_mobile::android::jni::platform()
                        .and_then(|platform| platform.primary_window())
                        .map(|win| {
                            let insets = win.safe_area_insets_logical();
                            xolhn_ui::SafeAreaInsets {
                                top: insets.top,
                                bottom: insets.bottom,
                                left: insets.left,
                                right: insets.right,
                            }
                        })
                        .unwrap_or_default()
                }),
            );
            gpui_kit::component::input::set_native_touch_selection(true, cx);

            match cx.open_window(
                gpui::WindowOptions {
                    window_bounds: None,
                    ..Default::default()
                },
                |window, cx| {
                    let view = xolhn_ui::create_root_view(window, cx);
                    cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
                },
            ) {
                Ok(window_handle) => {
                    let any_handle: gpui::AnyWindowHandle = window_handle.into();
                    cx.spawn(async move |cx: &mut gpui::AsyncApp| {
                        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let _stop_guard = SafeAreaWatcherStop(std::sync::Arc::clone(&stop));
                        let mut seen_gen = 0;
                        loop {
                            let stop_flag = std::sync::Arc::clone(&stop);
                            let next_gen = cx
                                .background_executor()
                                .spawn(
                                    async move { wait_for_safe_area_change(seen_gen, &stop_flag) },
                                )
                                .await;
                            let Some(generation) = next_gen else {
                                break;
                            };
                            seen_gen = generation;
                            if cx
                                .update_window(any_handle, |_, window, cx| {
                                    window.refresh();
                                    cx.refresh_windows();
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                    })
                    .detach();
                }
                Err(err) => {
                    log::error!("Failed to open XolHN Android window: {err:#}");
                }
            }
            cx.activate(true);
        });
}

#[cfg(not(target_os = "android"))]
pub fn android_platform_only() {}
