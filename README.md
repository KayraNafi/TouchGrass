# TouchGrass

Break reminders for humans glued to their chair. Tray-first, idle-aware nudges that make your spine breathe.

> Micro-breaks, macro-you.

TouchGrass lives in your tray, watches keyboard and mouse activity (never what you type), and taps your shoulder before your neck files a complaint. Tiny notifications, optional chimes, zero guilt trips.

## Grab a build

- Latest installers for Debian, Fedora, and Windows sit on the [Releases page](../../releases). No macOS build yet-I can’t stand that OS, but never say never.
- Already installed? TouchGrass checks for updates on launch and offers a one-click install.

## What makes it stick

- **Idle-aware timers** - reminders land only when you are actually working.
- **Tray-first control** - snooze, pause, tweak intervals, or pop settings straight from the tray.
- **Quick status** - next reminder, last break, idle streak, and test reminder all in one panel.
- **Opt-in chime** - toggle the audio prod if you want a little extra nudge.
- **Auto updates** - fetch and install new builds without leaving the app.
- **Autostart switch** - boot with the OS, skip the preference spelunking.

## How it behaves

1. Load preferences from `preferences.json` so interval, idle threshold, theme, and toggles survive restarts.
2. A background worker polls idle time every 20 s. If you wander off past the threshold, the timer resets when you return.
3. When the clock hits zero and you are active, TouchGrass fires a native notification, emits `touchgrass://reminder` for the chime, then schedules the next break.
4. Status events keep the Svelte UI and tray menu in sync-pause state, snooze end, and next trigger.

## Under the hood

| Layer          | Choice                           | Notes                                             |
| -------------- | -------------------------------- | ------------------------------------------------- |
| Desktop shell  | [Tauri](https://tauri.app)       | Native packaging, tiny footprint                  |
| Backend        | Rust (async)                     | Schedules timers, polls idle, bridges system APIs |
| Frontend       | Svelte + Vite                    | Fast boot, themeable components                   |
| Persistence    | JSON in app config dir           | Preferences stay local per user                   |
| Idle detection | [`user-idle2`](https://crates.io/crates/user-idle2) | Cross-platform idle polling                      |
| Notifications  | `tauri-plugin-notification`      | Native notification APIs                          |
| Autostart      | `tauri-plugin-autostart`         | One-liner launch at login                         |

## What you need installed

| Tooling | Notes |
| ------- | ----- |
| Rust    | Stable toolchain via `rustup` (1.70+ recommended) |
| Node.js | 18+ paired with `pnpm`                            |
| pnpm    | 8+                                                |

### Platform prep

TouchGrass depends on WebKitGTK plus tray packages. Install them before `pnpm tauri dev` or `cargo check`.

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

## Run it locally

```bash
# Toolchains (first run)
rustup default stable
cargo install tauri-cli

# JavaScript deps
pnpm install

# Desktop app in dev mode
pnpm tauri dev
```

The dev server spins up the Svelte frontend (`pnpm dev`) and hot reloads when you tweak code.

## Tweak your reminders

All settings persist and restore on launch.

- **Reminder interval** - presets (15/25/30/45/60/90) or your own number.
- **Idle threshold** - minutes of inactivity that count as a break (1–30).
- **Activity detection** - disable for a simple recurring timer.
- **Chime** - flip the sound on or off.
- **Autostart** - launch TouchGrass at login.
- **Theme** - dark by default, light if you insist.

Preferences live in the OS config dir: `~/Library/Application Support/touchgrass/preferences.json`, `~/.config/touchgrass/preferences.json`, or `%APPDATA%\touchgrass\preferences.json`.

## Dev shortcuts

```bash
pnpm dev         # Svelte-only dev server
pnpm tauri dev   # Desktop shell in dev mode
pnpm check       # Type-check Svelte components
pnpm build       # Build frontend bundle (used by Tauri prod builds)
pnpm tauri build # Produce native binaries (requires platform toolchains)
```

Rust helpers:

```bash
cargo check
cargo fmt
```

## Packaging flow

- Debian (`.deb`), Fedora (`.rpm`), and Windows (`.msi`/`.zip`) installers publish automatically on every `v*` tag.
- Maintainers: follow `docs/release-guide.md` when prepping releases or rotating signing keys.

## Roadmap snapshot

- [x] Idle-aware reminder loop with tray controls
- [x] Cross-platform builds with signed auto-updates
- [ ] macOS target and notarization
- [ ] Automated tests and smoke matrix
- [ ] More quick actions (custom snooze lengths, shortcuts)

## Contribute without being a jerk

Feedback, issues, and PRs welcome. Stick to the brand guidelines (`brand-guidelines.md`) and keep the tone playful, not mean.

1. Fork and clone the repo.
2. Create a topic branch.
3. Run `pnpm tauri dev` while you iterate.
4. Include `pnpm check` and `cargo check` output in your PR.

## License

Built with plenty of AI copiloting and zero gatekeeping. Use, remix, and ship it in whatever form you want. I won’t chase you down, and even if I changed my mind, the lawyer bill would flatten me.

---

Sure you are in your 20s? Your neck disagrees. TouchGrass is open source, built with Tauri and Svelte, and here to keep you from permanently fusing with the chair.
