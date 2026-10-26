# Achievement Watcher

Achievement notifications for Steam games running on emulators, rebuilt from scratch with Rust and Tauri.

Install it, forget about it: on first start it finds every game and achievement already on your PC, sits in the tray, starts with Windows, and shows an animated popup with sound and a screenshot the moment you unlock something. No API key, no folders to configure, no extra downloads.

> Remake of [Achievement Watcher](https://github.com/xan105/Achievement-Watcher) by Xan105 and the [2.x fork](https://github.com/darktakayanagi/Achievement-Watcher) by Darktakayanagi. Not affiliated with Valve or any emulator.

## Features

- **Automatic detection** of achievement files in emulator folders, Steam libraries, your own game folders and the folder of any game you open.
- **Real time**: file-system events with debounce plus a periodic safety check; an unlock becomes a popup in under a second.
- **No popup flood**: the first start imports everything silently and shows one summary; unlocks older than a few minutes are imported quietly.
- **Rare and platinum**: achievements owned by fewer than 10% of players play the rare sound, and the one that completes a game plays the platinum sound. Styles change colour for both.
- **Popup styles** (Default, Xbox One, Xbox 360, PlayStation 5 and PlayStation 4) in plain HTML/CSS/JS. Styles from Achievement Watcher 2.x still work when dropped in the styles folder.
- **Sound packs**: a folder with `unlock`, `rare` and `platinum` files, or a single audio file.
- **Progress popups** when a tracked achievement moves forward.
- **Overlay** with the current game's achievements, toggled with `Ctrl+Shift+A`.
- **Playtime** and "playing now", from the games running on your PC.
- **Steam client achievements** in the same library (no popup by default, Steam already shows one).
- **Screenshots** to `Pictures\<Game>\<Achievement>.png` and optional **video clips** of the moments before an unlock (ffmpeg is downloaded only if you turn it on).
- **English and Portuguese** interface, automatic updates, and a diagnostics page with a one-click export.
- **Offline friendly**: names, descriptions and icons come from Steam's public API without a key and are cached on disk.

## Supported emulators

| Emulator | Where it looks |
|---|---|
| OnlineFix | `Public\Documents\OnlineFix\<appid>` |
| Goldberg / GSE (gbe_fork) | `%APPDATA%\Goldberg SteamEmu Saves`, `%APPDATA%\GSE Saves`, `steam_settings` inside the game folder |
| CODEX / RUNE | `Public\Documents\Steam\{CODEX,RUNE}`, `%APPDATA%\Steam\CODEX` |
| EMPRESS | `%APPDATA%\EMPRESS`, `Public\Documents\EMPRESS` |
| RLD! / 3DM | `%PROGRAMDATA%\Steam\*\<appid>\stats`, `%PROGRAMDATA%\RLD!` |
| SKIDROW / Hoodlum | `Documents\{SKIDROW,Player}`, `%LOCALAPPDATA%\SKIDROW` |
| CreamAPI | `%APPDATA%\CreamAPI` |
| TENOKE | `SteamData\user_stats.ini` inside the game folder |
| RAZOR1911 | `%APPDATA%\.1911\<appid>\achievement` |
| SmartSteamEmu | `%APPDATA%\SmartSteamEmu\<appid>\stats.bin` |
| ALI213, Hoodlum, DARKSiDERS, Catherine, 3DM | marker files inside the game folder |
| Steam client | `Steam\appcache\stats` |

## Install

Download `Achievement.Watcher_<version>_x64-setup.exe` from the releases page and run it. It installs for the current user, no admin rights needed. Windows 10 22H2 or Windows 11.

The installer is not code-signed, so Windows SmartScreen may show "Windows protected your PC" the first time. Click **More info** and then **Run anyway**. After that the app updates itself from new releases.

Popups show over borderless and windowed games. Exclusive fullscreen hides any overlay that is not injected into the game.

## Custom styles and sounds

Settings has **Open styles folder** and **Open sounds folder** buttons (both live in `%APPDATA%\io.github.pedrosegato.achievementwatcher`).

A style is a folder with an `index.html` and a `preset.json`:

```json
{ "name": "My style", "author": "me", "version": "1.0.0", "width": 440, "height": 116, "duration": 6.5 }
```

The page receives each achievement through `window.api.onNotification(cb)` with `{ game, displayName, description, iconPath, scale, duration, position }`. Scripts must be inside the style folder; anything loaded from the internet is blocked.

## Releases and updates

The app checks `releases/latest/download/latest.json` 30 seconds after it starts and once a day, and installs a newer signed release by itself when no game is running.

Releases are built locally:

1. Bump the version in `crates/aw-app/tauri.conf.json`, the three `Cargo.toml` files and `ui/package.json`, and merge it into `main`.
2. Check out `main` and run `pnpm release`. It builds the installer at below-normal priority, signs it for the updater, writes `latest.json` and publishes the release with the GitHub CLI, tagged with the bare version (`3.0.1`).

The updater key is read from `TAURI_SIGNING_PRIVATE_KEY`, or from `~/.tauri/achievement-watcher.key` when that is not set. `pnpm release --notes notes.md` uses your own release notes instead of GitHub's generated ones, and `pnpm release --dry-run` stops before publishing, leaving the files in `target/release/upload`.

`.github/workflows/release.yml` builds the same files on GitHub Actions as a draft release, but only when started by hand from the Actions tab with the tag as input. It needs the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## Build from source

Requirements: stable Rust, Node 24 and pnpm 12.

```bash
pnpm install
pnpm tauri dev
pnpm tauri build
```

The installer lands in `target/release/bundle/nsis/`.

The detection core can be tested without the app:

```bash
cargo run -p aw-cli -- scan
cargo run -p aw-cli -- scan --games-root "C:\Games"
cargo run -p aw-cli -- watch
```

Checks: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm check`. The same checks run on GitHub Actions only when the `ci` workflow is started by hand.

## Repository layout

```
crates/aw-core            detection core: discovery, parsers, watcher, SQLite state, Steam metadata
crates/aw-cli             command line tool for the core
crates/aw-app             desktop app (Tauri): tray, popup window, sound, screenshot, commands
crates/aw-app/resources   built-in popup styles and sound packs
ui                        main window (Vite + React + TypeScript)
```

All Rust lives in `crates/` (one Cargo workspace) and the web front end in `ui/` (one pnpm workspace). `target/` and `node_modules/` are created by `cargo` and `pnpm` and are not part of the repository.

## License

[LGPL-3.0-only](./LICENSE). Credits and third-party notices in [NOTICE.md](./NOTICE.md).
