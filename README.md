# TouchGrass

> "Go touch grass." · "Micro-breaks, macro-you." · "Be less chair." · "You sure you are 20s?"

TouchGrass is a lightweight Tauri desktop companion that keeps you honest about your screen time. It lives in your tray, pops up playful break reminders, and adapts to your workflow with idle-aware scheduling, configurable cadences, and quick controls.

![TouchGrass settings window](docs/screenshot-settings.png)

## Download

- **Latest installers** – grab the current release for Debian, Fedora, or Windows from the [Releases page](../../releases).
- **Auto updates** – once installed, TouchGrass checks for updates when it launches and offers a one-click install inside the app.

## Features

- **Adaptive reminders** – choose from common presets or custom intervals; timers automatically restart when you have been idle for a user-defined window (default 2 min).
- **Idle detection** – watches keyboard/mouse activity only; no keystroke logging, no network calls.
- **Tray-first UX** – pause/resume, snooze (5/15 min), open settings, or quit directly from the tray menu.
- **Playful nudges** – native notifications with optional chime, plus the latest reminder message surfaced in the UI.
- **Dark by default** – theme token system with one-click light mode for the rebels.
- **Autostart toggle** – launch at login without digging through OS preference panes.

## Stack

| Layer              | Choice                           | Notes                                                 |
| ------------------ | -------------------------------- | ----------------------------------------------------- |
| Desktop shell      | [Tauri](https://tauri.app)       | Native packaging, tiny footprint, secure bridge       |
| Backend            | Rust (async)                     | Handles scheduling, idle detection, system APIs       |
| Frontend           | Svelte + Vite                    | Fast boot, component-driven UI, easy theming          |
| Persistence        | JSON in app config dir           | Preferences stored locally per user                   |
| Idle detection     | [`user-idle2`](https://crates.io/crates/user-idle2) | Cross-platform idle time polling                     |
| Notifications      | `tauri-plugin-notification`      | Native notification APIs per platform                 |
| Autostart          | `tauri-plugin-autostart`         | One-line launch-at-login integration                  |

## How it works

1. **Preferences** load from `preferences.json` and are cached in state (interval, idle threshold, toggles, theme).
2. A background worker waits for the next reminder, polling OS idle time every 20 s. If you have been idle longer than the configured threshold, the timer restarts when you resume activity.
3. When the timer elapses and you are active, TouchGrass fires a notification, emits a `touchgrass://reminder` event (UI chime), and schedules the next break.
4. Status events keep the Svelte UI and tray menu in sync-pause state, snooze end, next trigger timestamp, and the most recent idle seconds reading.

## Requirements

| Tooling | Notes |
| ------- | ----- |
| Rust    | Stable toolchain via `rustup` (1.70+ recommended) |
| Node.js | 18+ (paired with `pnpm`) |
| pnpm    | 8+ |

### Platform prerequisites

TouchGrass relies on WebKitGTK + tray dependencies. Install them before running `pnpm tauri dev` or `cargo check`.

<details>
<summary>Debian / Ubuntu</summary>

```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
  libgtk-3-dev libappindicator3-dev libayatana-appindicator3-dev \
  libxss-dev libdbus-1-dev
```

</details>

<details>
<summary>Fedora</summary>

```bash
sudo dnf install -y \
  webkit2gtk4.1-devel libsoup3-devel gtk3-devel \
  libappindicator-gtk3-devel libayatana-appindicator-gtk3-devel \
  libXScrnSaver-devel dbus-devel
```

</details>

<details>
<summary>Arch Linux</summary>

```bash
sudo pacman -S --needed webkit2gtk-4.1 gtk3 libappindicator-gtk3 libxss dbus
```

</details>

## Getting started

```bash
# Install toolchains (first run only)
rustup default stable
cargo install tauri-cli

# JS dependencies
pnpm install

# Launch the desktop app in dev mode
pnpm tauri dev
```

The Tauri dev server automatically spins up the Svelte frontend (`pnpm dev`) and hot reloads on source changes.

## Configuration

All settings persist automatically and restore on startup. Highlighted options:

- **Reminder interval** – presets (15/25/30/45/60/90) or custom minutes.
- **Idle threshold** – minutes of inactivity that qualify as a break (1–30); the timer restarts when you resume after a longer idle period.
- **Activity detection** – disable to behave like a simple recurring timer.
- **Chime** – toggle the notification sound.
- **Autostart** – launch TouchGrass at login.
- **Theme** – dark (default) or light UI.

Preferences live in the OS config dir, e.g. `~/Library/Application Support/touchgrass/preferences.json`, `~/.config/touchgrass/preferences.json`, or `%APPDATA%\touchgrass\preferences.json` depending on platform.

## Development scripts

```bash
pnpm dev         # Svelte-only dev server
pnpm tauri dev   # Desktop shell in dev mode
pnpm check       # Type-check Svelte components
pnpm build       # Build frontend bundle (used by Tauri prod builds)
pnpm tauri build # Produce native binaries (requires platform toolchains)
```

Rust helpers:

```bash
cargo check  # Validate the Rust backend
cargo fmt    # Format Rust sources
```

## Packaging & updates

- **Platforms** – Maintained installers for Debian (`.deb`), Fedora (`.rpm`), and Windows (`.msi`/`.zip`) publish automatically whenever a `v*` tag lands.
- **Maintainer note** – Follow `docs/release-guide.md` when preparing a new version or rotating signing keys.

## Roadmap snapshot

- [x] MVP reminder loop with idle-aware scheduling
- [x] Tray menu + settings UI + optional chime
- [x] Cross-platform packaging (Debian/RPM/Windows) with signed auto-updates
- [ ] macOS build target & notarization pipeline
- [ ] Automated tests & smoke-test matrix
- [ ] Additional quick actions (custom snooze lengths, shortcuts)

## Contributing

Feedback, issue reports, and pull requests are welcome. Please align with the brand guidelines (`brand-guidelines.md`) and keep the tone playful-sarcastic, never mean.

1. Fork & clone the repo.
2. Create a topic branch.
3. Run `pnpm tauri dev` to iterate locally.
4. Include `pnpm check` / `cargo check` output before opening a PR.

## License

License to be announced. All brand assets © TouchGrass contributors.

---

Built with Tauri because fast desktops deserve small footprints. Inspired by everyone whose spine deserves better. Your spine will thank you.
