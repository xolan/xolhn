# XolHN

An alternative cross-platform **Hacker News** reader client built in Rust for **Windows**, **Linux**, and **Android**.

## v1.0.0 Scope (MVP)

The v1.0.0 release is intentionally **read-only** with **no user login**:

- Browse Hacker News feeds (`Top`, `New`, `Best`, `Ask HN`, `Show HN`, `Jobs`)
- Search stories and discussions
- Read nested, collapsible comment threads with depth color rails
- Open story links and embedded comment links in the system browser
- Visual accessibility & typography controls: UI/reading font family & monospace font family selection (system + bundled open-source fonts such as Atkinson Hyperlegible, Inter, Literata/Newsreader, and JetBrains Mono), base font size scaling (`rem`), line-height density, and desktop AccessKit semantics

## Locked-In Tech Stack

- **Language & UI:** Rust + GPUI (`gpui-pre = "=0.3.10"`) + `gpui-kit = "=0.8.1"` (desktop AccessKit semantics, `TextView`, built-in Catppuccin & Gruvbox themes)
- **Android Platform Layer:** [`longbridge/gpui-mobile`](https://github.com/longbridge/gpui-mobile) (`gpui-pre-mobile`, `rev = "7548ebc4b24f00494af9976af8852c7163ded9ce"`, Vulkan / GL ES 3.0 on Android, built-in `open_url` via `ACTION_VIEW`)
- **API (v1.0.0):** Single-API **Algolia HN Search API** (`https://hn.algolia.com/api/v1/`) behind an `HnApi` trait
- **Planned Workspace Layout:**
  - `xolhn-core` — Algolia API client (`HnApi`), local cache, HN HTML parser, theme palettes, typography & accessibility settings
  - `xolhn-ui` — Shared responsive GPUI + `gpui-kit` views
  - `xolhn-desktop` — Windows/Linux desktop binary (`gpui-pre-platform`)
  - `xolhn-android` — Android `cdylib` (`gpui-pre-mobile`) + Gradle host

## Theme Suite

- **XolHN Dark** (branded dark default)
- **XolHN Light** (branded warm light)
- **Rosé Pine**
- **Catppuccin** (Mocha & Latte)
- **Gruvbox** (Dark)

## Development Setup (`mise`)

All toolchains (Rust + `aarch64-linux-android` & `x86_64-linux-android`, Temurin JDK 17, Android SDK/NDK, `cargo-ndk`, `adb`, Android Emulator) and build tasks are managed declaratively via [mise.toml](mise.toml):

```fish
mise trust
mise run setup
mise run doctor
```

*(Optional CachyOS host setup if deploying over USB to a physical Android device: `sudo pacman -S android-udev && sudo usermod -aG adbusers $USER`.)*

### Common Tasks

| Command | Purpose |
| :--- | :--- |
| `mise run setup` | Install all tools and Android SDK/NDK components |
| `mise run setup:emulator` | Install Android Emulator, API 34 system image, and create default AVD |
| `mise run doctor` | Verify desktop & Android toolchains (`rustc`, `java`, `cargo-ndk`, NDK, `adb`, emulator/AVDs) |
| `mise run check` | Type-check desktop + Android targets and run `xolhn-core` tests |
| `mise run fmt` | Format all Rust crates with `rustfmt` |
| `mise run lint` | Run `rustfmt --check`, `clippy --all-targets`, and `actionlint` |
| `mise run test` | Run workspace unit tests |
| `mise run ci` | Run desktop CI checks (`lint` and `test`) |
| `mise run ci:android` | Run Android NDK `clippy` (`arm64-v8a`, `x86_64`) and debug APK build |
| `mise run dev:desktop` | Run the Windows/Linux desktop app |
| `mise run dev:android` | Build, install, and launch XolHN on an Android emulator (auto-boots AVD) |
| `mise run build:desktop` | Build the release desktop binary |
| `mise run build:android` | Compile `xolhn-android` (`arm64-v8a`) and assemble debug APK |
| `mise run build:android-release` | Build release Android APK |
| `mise run package <target>` | Package a release archive (`tar.gz`/`zip`) and `.sha256` into `dist/` |
| `mise run package:android` | Package release `arm64-v8a` APK and `.sha256` into `dist/` |
| `mise run install:android` | Build and install debug APK via `adb` |
| `mise run logs:android` | Stream `xolhn` Android logcat output |

### CI & Releases

GitHub Actions workflows live in [.github/workflows/ci.yml](.github/workflows/ci.yml) (Linux and Windows) and [.github/workflows/release.yml](.github/workflows/release.yml) (auto-releases `v<version>` on `main` when the workspace version in [Cargo.toml](Cargo.toml) has no matching git tag yet). Android builds are skipped on CI for now and built/sideloaded locally.

## Sideloading on a Physical Android Device

Physical Android phones (`arm64-v8a`, Android 8.0 / API 26+) can be installed either directly over USB with `adb` or by copying a packaged APK onto the device.

### 1. Prerequisites

1. Install the local toolchains and Android SDK/NDK if you haven't already:
   ```fish
   mise run setup
   ```
2. On your Android device, enable **Developer options** (*Settings → About phone → tap Build number 7 times*), then enable **USB debugging** (*Settings → System → Developer options → USB debugging*) if installing via `adb`.
3. On Arch/CachyOS Linux hosts, ensure USB `adb` permissions are configured (`sudo pacman -S android-udev && sudo usermod -aG adbusers $USER`, then log out and back in).

### 2. Option A: Install over USB (`adb`)

1. Connect the phone via USB, accept the **Allow USB debugging** prompt on the device, and verify it appears as `device`:
   ```fish
   mise exec -- adb devices
   ```
2. Build and install the release APK (recommended for day-to-day use):
   ```fish
   mise run build:android-release
   mise exec -- adb -d install -r android/app/build/outputs/apk/release/app-release.apk
   ```
   > By default, `assembleRelease` falls back to debug-keystore signing so the APK can be sideloaded directly. To sign with your own release keystore instead, export `ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, and `ANDROID_KEY_PASSWORD` before building.
3. Alternatively, for quick debug testing:
   ```fish
   mise run install:android
   ```

**Troubleshooting `adb` installs:**
- If both an emulator and a physical phone are attached, `-d` targets the USB device, or pass `-s <serial>` from `adb devices`.
- If switching between builds signed with different keys (for example, debug-signed vs. custom release keystore), uninstall the existing `dev.xolhn.app` package first:
  ```fish
  mise exec -- adb -d uninstall dev.xolhn.app
  ```
- Optional runtime log inspection:
  ```fish
  mise run logs:android
  ```

### 3. Option B: Install APK Directly on the Phone (No USB Debugging)

1. Build and package a versioned `arm64-v8a` APK into `dist/`:
   ```fish
   mise run package:android
   ```
2. Transfer `dist/xolhn-<version>-android-arm64-v8a.apk` to your phone (USB file transfer, LocalSend, etc.).
3. Tap the `.apk` file in the phone's file manager and allow **Install unknown apps** when prompted.

## Project Documents

- [Architecture & Handoff](docs/architecture.md) — Locked-in tech stack, crate alignment (`gpui-pre = "=0.3.10"`), workspace layout, and verified `mise` toolchain state.
- [API Research & Architecture](docs/api-research.md) — Evaluation of free Hacker News APIs, rate limits, caveats, and the v1.0.0 Algolia-only decision.
- [Desktop & Mobile UI Mockups](docs/mockups/xolhn-mockups.html) — Interactive HTML prototype with live theme switching for Desktop (Windows/Linux) and Mobile (Android).
