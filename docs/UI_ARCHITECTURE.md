# UI Architecture Decision Draft

Status: architecture spike plan. This document intentionally separates product requirements from implementation choice. Do not treat the renderer choice as final until the local prototypes described below have been tested on Windows and Linux/Wayland.

## Core requirement

The future application should behave as one product on Windows and Linux:

- lightweight native supervisor/UI process;
- overlay summoned over PoE2;
- existing proven Windows automation backend retained;
- Linux launches that backend through PoE2's active Proton environment using `runinprefix`;
- UI and backend communicate through a small IPC protocol;
- UI remains available while PoE2 is closed and reconnects automatically when the game starts again.

The backend remains authoritative for memory reads, flask decisions, input, area/focus gating, and automation state.

## Proposed process model

```text
Windows

poe2-auto-flask UI/supervisor
        |
        | spawn + IPC
        v
poe2-auto-flask backend.exe
        |
        v
PathOfExileSteam.exe
```

```text
Linux

poe2-auto-flask native UI/supervisor
        |
        | discover running PoE2 + active Proton
        | spawn through proton runinprefix
        v
poe2-auto-flask backend.exe under Proton/Wine
        |
        v
PathOfExileSteam.exe under same Proton environment
```

When PoE2 exits, the backend may exit as it does today, but the native UI/supervisor should stay alive and wait for the next game launch.

## IPC direction

Use the same IPC design on both operating systems if practical.

Preferred first option: loopback TCP on `127.0.0.1` with an ephemeral port and per-session random token.

Reasons:

- works natively on Windows;
- crosses the Wine/host boundary cleanly on Linux;
- avoids platform-specific named-pipe/Unix-socket duplication;
- traffic volume is tiny;
- easy to debug during development;
- can bind only to loopback and authenticate every session.

The UI/supervisor should create the listener and launch the backend with the endpoint and random token through private command-line arguments or environment variables. The backend connects back to the UI.

Initial protocol can use newline-delimited JSON because throughput is negligible and human-readable traces are useful during development. A binary protocol is unnecessary unless measurements later justify one.

### Example backend-to-UI events

- `hello` / protocol version;
- connected/disconnected;
- vitals changed;
- area changed;
- focus changed;
- automation state changed;
- flask state/charges changed;
- bindings changed;
- config reloaded;
- backend error/warning;
- backend shutting down.

### Example UI-to-backend commands

- arm;
- disarm;
- set Life enabled/threshold;
- set Mana enabled/threshold;
- set hideout behavior;
- request full state snapshot;
- graceful shutdown.

The exact schema should be versioned from the beginning even if version 1 is small.

## Configuration ownership

The existing TOML file remains the user's durable source of truth.

Linux:

- `$XDG_CONFIG_HOME/poe2-auto-flask/config.toml`
- fallback `~/.config/poe2-auto-flask/config.toml`

Windows:

- `%APPDATA%\poe2-auto-flask\config.toml`

Preferred long-term design:

- extract config schema/validation into a small shared Rust module/crate used by the native UI host and backend;
- UI writes configuration atomically;
- backend continues watching/reloading the same file;
- backend emits config-reloaded state back to the UI;
- manual edits remain fully supported.

Do not introduce SQLite or a private UI settings database for settings already represented in TOML.

## Renderer candidates

Two candidates are worth serious local prototyping.

### Candidate A: Tauri 2 + small web frontend

Suggested frontend if selected: Svelte or similarly small compiled frontend; avoid a large component framework unless it provides measurable value.

Strengths:

- easiest path to match the selected polished mockup precisely;
- CSS makes rounded cards, gradients, opacity, transitions, disabled states, sliders, and responsive layout straightforward;
- very fast visual iteration;
- Rust native host fits the existing codebase and can own process discovery, IPC, config, tray, and platform integration;
- Tauri uses the system webview rather than bundling a full browser runtime;
- good cross-platform packaging story.

Costs/risks:

- Linux uses WebKitGTK and Windows uses WebView2, so runtime footprint is larger than a pure native renderer and rendering differences must be tested;
- Tauri's standard global-shortcut stack currently relies on `global-hotkey`, whose Linux implementation is X11-only;
- a custom Wayland global-shortcut implementation is therefore required for KDE/Wayland today;
- true overlay positioning/activation on Wayland must be proven rather than assumed.

### Candidate B: Slint + Rust

Strengths:

- native Rust UI with very low runtime footprint;
- declarative UI and live preview are well suited to visual iteration;
- custom styling, animations, responsive layouts, tooltips, and tray support are available;
- keeps the application almost entirely Rust;
- likely lowest CPU/RAM overhead of the polished candidates.

Costs/risks:

- matching the selected design will require more custom component styling than CSS/Tauri;
- fewer ready-made web-style layout/component patterns;
- global shortcut and Wayland overlay behavior are still platform problems outside the renderer, so Slint does not eliminate the hardest Linux integration work;
- desktop feature maturity should be verified against our exact requirements.

### Candidate C: egui/eframe

Not preferred for the first prototype.

It is lightweight and Rust-native, but the immediate-mode styling model would require more custom work to reach the selected polished product appearance. Keep it as a fallback if Tauri and Slint expose blockers.

### Qt/GTK/Electron

Not preferred:

- Electron conflicts with the lightweight goal;
- Qt/GTK add framework/deployment weight without giving a clear advantage for this custom overlay;
- native toolkit styling is less directly aligned with the selected visual design.

## Wayland is a first-class architecture constraint

The development host is KDE Plasma on Wayland, so an X11-only solution is not acceptable.

Important current constraint:

- Tauri's `global-hotkey` dependency documents Linux support as X11-only.

Preferred Linux/Wayland hotkey path:

- implement the standard `org.freedesktop.portal.GlobalShortcuts` XDG Desktop Portal interface from the native Rust host;
- request an `Open overlay` action with F10 as the preferred trigger;
- listen for Activated/Deactivated portal signals;
- retain the user's compositor-managed binding;
- use the activation token returned with a shortcut event when useful for legally bringing the overlay window to the foreground under Wayland.

KDE's XDG portal implementation supports the Global Shortcuts portal, so this should be tested directly on CachyOS/Plasma rather than replaced with X11 compatibility hacks.

Windows can use the native/global-hotkey path.

Linux/X11 can use the normal global-hotkey implementation as a fallback.

## Overlay window model

Do not assume a Steam-style injected overlay. The intended first implementation is a separate native top-level overlay window owned by poe2-auto-flask.

Desired behavior:

- invisible while closed;
- summoned by global shortcut;
- appears on the monitor containing PoE2;
- borderless and visually transparent around the designed panel;
- covers enough of the game surface to provide a dim backdrop;
- receives keyboard/mouse focus while open;
- closes with F10/Escape;
- returns focus to PoE2 cleanly;
- does not affect game rendering or inject into the game process.

Potential Wayland issue:

- normal Wayland clients do not have unrestricted control over absolute positioning, stacking, or focus.

Therefore the architecture spike must prove the following on KDE Wayland before renderer choice is final:

1. global shortcut opens the overlay while PoE2 is focused;
2. overlay appears above PoE2 reliably;
3. overlay receives focus/input;
4. background dim layer works correctly;
5. closing returns focus to PoE2;
6. behavior still works when PoE2 is borderless/fullscreen as normally used;
7. no XWayland-only dependency is accidentally required.

If a normal top-level window cannot meet this reliably, investigate a Linux-specific layer-shell/window integration while keeping the renderer-independent application architecture intact.

## Recommended decision process

Do not choose Tauri or Slint from documentation alone. Build two very small local spikes before writing the full UI.

### Spike A - Tauri

Only implement:

- dark transparent/borderless window;
- one centered rounded panel;
- 30-40% backdrop dim;
- 160 ms slide/fade animation;
- one toggle, slider, numeric input, and Set button;
- F10 open/close;
- Escape close;
- no backend integration yet.

Measure:

- idle hidden CPU/RAM;
- overlay-open CPU/RAM;
- startup time;
- animation smoothness;
- Wayland focus/stacking behavior;
- effort required to match the mockup.

### Spike B - Slint

Implement the exact same shell and controls, then make the same measurements.

### Selection rule

Choose Tauri if:

- its resource footprint is still comfortably small;
- Wayland overlay behavior is reliable;
- visual implementation is materially faster/easier.

Choose Slint if:

- it reproduces the selected design without excessive custom work;
- its significantly lower runtime footprint is measurable;
- overlay behavior is at least as reliable as Tauri.

Do not optimize for binary size alone. The important measurements are runtime CPU/RAM, responsiveness, implementation complexity, and platform reliability.

## Provisional preference

Tauri 2 is the visual/productivity favorite for the first spike because CSS maps naturally to the selected mockup. Slint is the performance/footprint favorite and should receive an equal small prototype before the final decision.

The renderer choice must not affect the backend/IPC design described above.

## Backend evolution plan

Preserve normal/headless behavior while adding a service/IPC mode.

Suggested staged implementation:

1. Define shared IPC message types and protocol version.
2. Add an optional backend IPC mode without changing current automation behavior.
3. Emit snapshots/events while keeping terminal logging intact for debugging.
4. Add UI commands for config/state control.
5. Move Linux process supervision from the current launcher into the new native UI host.
6. Keep the existing CLI/launcher path available until the GUI path has a full regression pass.
7. Only then simplify packaging around the GUI application.

At no point should UI work require rewriting the proven memory-reading/input backend.

## Packaging direction

Linux:

```text
AppImage
|- native UI/supervisor
|- bundled Windows backend payload
|- UI assets
`- licenses
```

The UI/supervisor manages the backend copy as the current AppImage does and launches it with `proton runinprefix` after discovering the live game environment.

Windows:

```text
Windows package
|- native UI/supervisor
|- Windows backend sidecar
|- UI assets
`- licenses
```

Keeping the backend as a sidecar on both operating systems gives the closest behavioral symmetry and reduces platform-specific backend divergence.

## Update architecture

First GUI version only checks for updates and opens the GitHub release page.

Do not add self-update until both package formats and rollback behavior are intentionally designed.

## Performance principles

- hidden overlay should not continuously repaint;
- backend polling remains independent of UI rendering;
- UI receives event/state updates at an appropriate cadence rather than mirroring the backend's tight loop;
- avoid continuous animations while idle;
- keep telemetry rendering at a low useful rate;
- measure combined UI + backend footprint on both operating systems.

## First architecture milestone

The next implementation milestone is not the full UI. It is the renderer/Wayland spike:

1. install local development tooling;
2. create isolated Tauri and Slint overlay-shell prototypes;
3. test both on CachyOS KDE/Wayland over a running PoE2 session;
4. record CPU, RAM, startup, animation, focus, and hotkey behavior;
5. choose the renderer;
6. then implement the shared IPC layer and real UI incrementally.
