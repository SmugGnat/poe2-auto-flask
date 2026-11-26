# AGENTS.md

This repository contains PoE Auto Flask for Path of Exile 1 and 2 on Windows
and Linux/Steam Proton. The Windows application combines the Slint UI and the
proven Rust automation backend in one executable. Linux runs that executable
through the native launcher in the selected game's active Proton environment.

## Before making changes

- Read `app-ui/README.md` for UI architecture, lifecycle and presentation rules.
- Read `docs/PROFILES.md` when changing settings or persistence, and
  `docs/SOURCE_LAYOUT.md` for module ownership.
- Read the relevant source before changing behavior. Code is authoritative.
- Use the official Slint skill when reviewing or editing `.slint` files or
  their Rust integration. Compile changes and inspect a render for UI edits.

## Branch and workflow

- Work on `develop` unless explicitly told otherwise. Keep an explicitly
  requested feature branch; do not switch branches without authorization.
- Keep changes focused. Iterate locally before using GitHub Actions.
- Run relevant formatting, Clippy, tests and local builds before pushing.
- Build validation runs on `develop`; promote to `main` only after review
  and validation. Release publishing is a separate, explicit operation.
- Do not create or move release tags unless explicitly requested.
- See the build and release instructions in `README.md` and `app-ui/README.md`.

## Proven backend constraints

- The existing Windows Rust backend remains authoritative for automation.
- Game memory is read-only. Input uses the existing validated dispatcher.
- Do not add memory writes, injection, drivers, services or root/admin requirements.
- Preserve fail-closed behavior, focus/area/chat gating and stale-state rejection.
- Avoid changing memory readers, classification, input, bindings or flask
  decisions unless the task requires it.
- The automation key defaults to F11 and is configurable in Settings. It only
  toggles automation while the selected game is focused. Startup, profile
  switches and automation-key changes leave automation disabled.

## UI architecture

- Slint owns the main thread; automation runs on its own Rust thread.
- Use bounded Rust channels for owned status snapshots and lifecycle commands.
  Do not introduce a separate helper process or IPC service.
- Presentation telemetry uses an independent read-only session outside the
  automation loop. It cannot authorize input or block automation.
- Hidden/minimized presentation stops timers and telemetry and caches backend
  changes without scheduling UI updates. Restore preserves unsaved drafts.
- Closing to tray continues automation; explicit Quit stops the application.
- Native Windows can wait/reconnect. Under Proton, exit with the game so Steam
  can clean up the game session.

## Linux/Proton

- Use `proton runinprefix` only. Never use `proton run` against a live game
  prefix; it caused game instability during testing.
- Discover the live Steam root, compatdata path and selected Proton from the
  running game. Do not require Protontricks or Steam Launch Options.
- Do not install the application into the game prefix. The AppImage maintains
  a host-side executable copy under the user's data directory.
- Preserve the opaque native window and the scoped Wine fullscreen stacking
  workaround. Linux Wayland/KWin is a primary testing environment.

## Configuration

- Linux: `$XDG_CONFIG_HOME/poe2-auto-flask/`, falling back to
  `~/.config/poe2-auto-flask/`.
- Windows: `%APPDATA%\poe2-auto-flask\`.
- Global preferences and window placement live in `settings.toml`;
  automation settings live in `profiles/*.toml`.
- Use the shared `ProfileStore` and validators. Preserve atomic saves,
  external-edit conflict detection and manual TOML editing.
- Legacy `config.toml` migrates without modification or deletion. It is not
  live configuration after migration. Keep the supported console/debug legacy
  override separate from the GUI's profile store.

## Toolchain and packaging

- All three crates inherit Rust 1.100.0 from `rust-toolchain.toml`.
- Slint and slint-build are pinned together. Keep toolchains and locked builds
  aligned across the backend, UI, Linux launcher and CI.
- Public assets are `poe-auto-flask.exe`, `poe-auto-flask.AppImage` and
  `LICENSES.txt`.
- Build and Release use the same packaging scripts. Normal launch opens the
  UI without a console; `--debug` retains read-only console diagnostics.
- The root Cargo package is the application version source. License collection
  must cover locked dependencies, Slint's license directories and Inter.

## Style

Prefer straightforward Rust, small dependencies and concise public wording.
Avoid unnecessary abstractions, background services and continuously running
presentation work.
