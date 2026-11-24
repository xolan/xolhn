# XolHN v1.0.0 Architecture & Handoff

## 1. Product Scope (v1.0.0 MVP)

- **Platforms:** Windows, Linux, Android.
- **Mode:** Strictly **read-only** reader (no user login, no upvoting, no posting/replying).
- **Core Capabilities:**
  - Browse feeds: `Top`, `New`, `Best`, `Ask HN`, `Show HN`, `Jobs`
  - Search stories & comments
  - Read collapsible nested comment trees with depth color rails
  - Open external story links and links inside comments in the system browser
- **Themes:**
  - **XolHN Dark** *(branded dark default)* & **XolHN Light** *(branded warm light)*
  - **Rosé Pine**
  - **Catppuccin** *(Mocha & Latte)*
  - **Gruvbox** *(Dark)*
- **Accessibility & Typography:**
  - UI/reading font family & monospace font family pickers (system fonts on desktop + bundled open-source fonts across all platforms, e.g., Atkinson Hyperlegible, Inter, Literata/Newsreader, JetBrains Mono, registered via `cx.text_system().add_fonts(...)`)
  - Base font size scaling (`rem` via `Window::set_rem_size` / `Theme::update`)
  - Line-height density options
  - Desktop AccessKit semantics via `gpui-kit` and Android TalkBack via `accesskit_android 0.7.5` (bundled in `longbridge/gpui-mobile`)

---

## 2. Locked-In Crate Alignment (`gpui-pre = "=0.3.10"`)

Keep every GPUI crate aligned to the exact same `gpui-pre` snapshot so `App` and `Window` types match across the workspace:

```toml
gpui = { package = "gpui-pre", version = "=0.3.10", default-features = false }
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.10" } # Windows / Linux
gpui-kit = { git = "https://github.com/longbridge/gpui-kit", rev = "ae6a3fdfef416e7040e773415ac9cbd8c623e5d5", default-features = false, features = ["component"] }
gpui-mobile = { package = "gpui-pre-mobile", git = "https://github.com/longbridge/gpui-mobile", rev = "7548ebc4b24f00494af9976af8852c7163ded9ce" }
```

*(Note: `gpui-kit` `0.8.1` on crates.io also pins `gpui-pre = "=0.3.10"`; `ae6a3fdf` matches `longbridge/gpui-mobile` `7548ebc4` and includes mobile `input::set_native_touch_selection` and mobile tooltip suppression.)*

### Android Host Details (`longbridge/gpui-mobile`)

- Rendering: Vulkan preferred, GL ES 3.0 fallback via `wgpu`; `cosmic-text` + `swash` text shaping.
- Built-in URL handling (`open_url` delegates to host or falls back to `ACTION_VIEW`), clipboard, path/preferences helpers, and `accesskit_android 0.7.5`.
- Gradle baseline in upstream example: Gradle `9.4.1`, `compileSdk = 34`, `minSdk = 26`, `targetSdk = 34`, `arm64-v8a` (`aarch64-linux-android`).

---

## 3. API Decision (v1.0.0: Algolia-Only)

Use `https://hn.algolia.com/api/v1/` exclusively in v1.0.0 behind a clean `HnApi` trait in `xolhn-core` (see [docs/api-research.md](api-research.md) for endpoint mappings, local caching strategy, and trade-offs).

---

## 4. Planned Workspace Structure

```text
/home/xolan/dev/xolhn
├── Cargo.toml                  # Workspace root
├── mise.toml                   # Toolchains + dev/build/check tasks
├── crates/
│   ├── xolhn-core/             # Algolia HnApi client, cache, HTML parser, themes, typography settings
│   ├── xolhn-ui/               # Shared responsive GPUI + gpui-kit views & components
│   ├── xolhn-desktop/          # Windows / Linux binary entrypoint (gpui-pre-platform)
│   └── xolhn-android/          # Android cdylib entrypoint (gpui-pre-mobile)
└── android/                    # Gradle host wrapping lib xolhn_android.so
```

---

## 5. Local Environment & Development Workflow

All desktop and Android toolchains are already configured and verified via [mise.toml](../mise.toml) (`mise run doctor` passed):

- `rustc 1.100.0` with `x86_64-unknown-linux-gnu` and `aarch64-linux-android` targets (`clippy`, `rustfmt`)
- Temurin JDK `17.0.21.1+2`
- `cargo-ndk 4.1.2`
- Android SDK `23.0` (`platforms;android-34`, `build-tools;34.0.0`, `platform-tools`, `adb 37.0.3`, `ndk;27.2.12479018`)

User workflow preference: work autonomously and rely heavily on cheap subagents (`flash` / `flash_lite`) for parallel crate/module implementation, research, and checks.
