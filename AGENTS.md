# AGENTS.md

This repository contains `poe2-auto-flask`, a small Path of Exile 2 auto-flask helper for Windows and Linux/Proton.

## Before making changes

Read these documents when working on the UI/overlay:

- `docs/UI_SPEC.md`
- `docs/UI_ARCHITECTURE.md`

Also read the relevant existing source before changing behavior. Do not infer backend behavior from the UI documents alone.

## Branch and workflow

- Work on `develop` unless explicitly told otherwise.
- Keep changes focused and easy to review.
- Iterate locally first; GitHub Actions is a validation gate, not the primary development loop.
- Run the relevant formatter, Clippy checks, tests, and local build before pushing code changes.
- Do not create or move release tags unless explicitly requested.

## Proven backend constraints

- The existing Windows helper is the proven automation backend.
- It reads PoE2 memory read-only and sends flask input through the existing input path.
- Do not add memory writes, injection, drivers, services, or elevated/root requirements.
- Preserve fail-closed behavior.
- Avoid changing memory-reading, area-classification, input, binding, or flask-decision code unless the task specifically requires it.

## Linux/Proton constraints

- Linux runs the Windows backend inside PoE2's active Proton environment.
- The proven launch mode is `proton runinprefix`.
- Never use `proton run` against the live PoE2 prefix; it caused game instability during testing.
- Do not require Protontricks or Steam Launch Options.
- The launcher discovers the live Steam root, compatdata path, and selected Proton from the running PoE2 environment.
- Do not install the helper into the PoE2 prefix.

## Configuration

Use the same user-editable configuration model on both operating systems:

- Linux: `$XDG_CONFIG_HOME/poe2-auto-flask/config.toml`, falling back to `~/.config/poe2-auto-flask/config.toml`
- Windows: `%APPDATA%\poe2-auto-flask\config.toml`

The future GUI should read/write the same configuration rather than introducing a second settings database. Manual edits must remain supported.

## UI architecture principles

- The backend remains authoritative for automation state and game data.
- The UI owns presentation, overlay interaction, and settings editing.
- Use IPC/event messages between UI and backend rather than duplicating game-memory logic in the UI.
- Keep the UI lightweight at idle and while hidden.
- Visual quality must be achievable without sacrificing responsiveness or system footprint.
- Windows and Linux should present the same product behavior where platform constraints allow it.
- Linux Wayland is a first-class target; do not assume X11-only global-hotkey behavior is sufficient.

## Current UX invariants

- F11 currently arms/disarms automation while PoE2 is focused.
- The future overlay hotkey is separate; F10 is the initial candidate, not yet a permanent choice.
- Town/hideout/focus gating and transient `WAITING FOR GAME STATE` behavior must remain safe.
- Closing the future overlay should not implicitly terminate automation; quitting the app must be a separate explicit action.

## Packaging

Current public release assets are:

- `poe2-auto-flask-windows-x64.exe`
- `poe2-auto-flask-x86_64.AppImage`
- `LICENSES.txt`

The Linux AppImage currently bundles the Windows backend and manages a host-side backend copy under the user's data directory.

## Style

- Prefer straightforward Rust and small dependencies.
- Avoid unnecessary abstraction and background services.
- Keep logs/messages concise and useful.
- Public-facing wording should be simple and direct.
