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

GitHub Actions workflows live in [.github/workflows/ci.yml](.github/workflows/ci.yml) (Linux, Windows, Android) and [.github/workflows/release.yml](.github/workflows/release.yml) (auto-releases `v<version>` on `main` when the workspace version in [Cargo.toml](Cargo.toml) has no matching git tag yet).

For production Android release signing, configure these repository secrets (`Settings -> Secrets and variables -> Actions`); when unset, release builds fall back to debug keystore signing:

- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

## Project Documents

- [Architecture & Handoff](docs/architecture.md) — Locked-in tech stack, crate alignment (`gpui-pre = "=0.3.10"`), workspace layout, and verified `mise` toolchain state.
- [API Research & Architecture](docs/api-research.md) — Evaluation of free Hacker News APIs, rate limits, caveats, and the v1.0.0 Algolia-only decision.
- [Desktop & Mobile UI Mockups](docs/mockups/xolhn-mockups.html) — Interactive HTML prototype with live theme switching for Desktop (Windows/Linux) and Mobile (Android).
