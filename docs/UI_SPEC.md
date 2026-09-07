# UI Specification

Status: first-pass product/UI specification for the planned poe2-auto-flask GUI and overlay.

The selected visual direction is a dark, modern in-game overlay with a Discord-like palette, rounded cards, restrained violet/blue accents, a dimmed game background, and fast slide/fade transitions. The UI must remain light, responsive, and direct.

## Goals

The UI should let the user:

- see whether Path of Exile 2 is connected;
- see current HP, Mana, Energy Shield, area, focus, and automation state;
- arm/disarm flask automation;
- enable or disable Life and Mana flask automation;
- adjust Life and Mana thresholds quickly and precisely;
- edit secondary options such as hideout behavior;
- see detected flask bindings and useful status information;
- see whether an application update is available;
- make these changes without leaving PoE2.

The experience should be consistent on Windows and Linux where platform behavior permits it.

## Interaction model

The primary UI is an overlay summoned over PoE2 with a dedicated global hotkey.

Initial hotkey candidate:

- F11 remains Arm / Disarm.
- F10 is the initial candidate for Open / Close Overlay.

F10 is provisional until conflicts with PoE2, Steam, and common PoE tools are checked. The overlay hotkey should ultimately be configurable.

When the overlay is open:

- pressing the overlay hotkey again closes it;
- Escape closes it;
- mouse and keyboard input belongs to the overlay while editing controls;
- text-field input must not leak through to PoE2;
- F11 may continue to arm/disarm unless testing shows that is undesirable;
- closing the overlay should return focus cleanly to PoE2.

Closing the overlay is not the same as quitting poe2-auto-flask. Quitting must remain a separate explicit action.

## Opening and closing

Opening should combine a short slide from the top with a fade.

Target opening behavior:

- roughly 140-180 ms;
- approximately 20-40 px vertical travel;
- opacity from 0 to 100%;
- background dim from 0 to roughly 30-40%.

Closing should be slightly faster, roughly 100-140 ms.

Animation must never block useful interaction for a noticeable period. Avoid bounce/spring effects. A future reduced-motion mode should use a short fade instead.

## Background treatment

The game remains visible while the overlay is open. A full-screen translucent dark layer should dim the gameplay view enough to improve readability while preserving context.

Blur is optional and must only be used if it is inexpensive and behaves consistently across supported platforms.

## Layout

The selected first-pass layout is a wide panel near the upper-middle portion of the active PoE2 monitor, with a separate compact telemetry strip beneath it.

Header:

- application icon and `poe2-auto-flask` title;
- connection status;
- large automation-state pill;
- segmented navigation.

Primary content:

- side-by-side Life Flask and Mana Flask cards on the Thresholds page.

Bottom telemetry strip:

- HP;
- Mana;
- ES;
- Area;
- CPU;
- RAM;
- Update status.

The interface should scale sensibly from 1920x1080 through 1440p, ultrawide, and 4K without becoming tiny or excessively wide.

## Visual language

Direction:

- dark charcoal/slate backgrounds;
- dark rounded cards;
- subtle borders and shadows;
- violet/indigo/blue primary accent;
- muted green success state;
- amber update/warning state;
- red Life accent;
- blue Mana accent;
- cyan/pale-blue ES accent.

Approximate shape language:

- outer panel radius around 16 px;
- cards around 12 px;
- buttons around 8-10 px;
- inputs around 8 px.

Use modern sans-serif typography with clear hierarchy and no unnecessarily small text.

## Header states

Connection examples:

- `Connected` / `PoE2 detected`;
- `Waiting for PoE2`;
- error state only when there is an actionable problem.

Automation-state pill examples:

- `DISARMED`;
- `ARMED`;
- `PAUSED - hideout`;
- `PAUSED - town`;
- `PAUSED - PoE2 not focused`;
- `WAITING - game state changing`.

The backend state is authoritative. The GUI must not invent a separate automation state machine.

## Navigation

Initial segmented navigation:

- Overview;
- Thresholds;
- Status;
- Updates.

The Thresholds page corresponds most closely to the selected visual mockup.

## Flask cards

Each flask card contains:

- Life or Mana icon/color identity;
- title;
- short description;
- enable/disable toggle;
- threshold slider;
- floating percentage value on the slider;
- numeric threshold input;
- `%` suffix;
- `Set` button.

### Enable toggle

Enabled:

- accent-colored switch;
- controls fully interactive.

Disabled:

- muted switch/card treatment;
- slider disabled;
- numeric input disabled;
- Set button disabled;
- optional concise helper text such as `Disabled - enable to adjust mana flask settings.`

Simple boolean settings such as `Enable in hideout` apply immediately and do not need a Set button.

## Threshold slider and numeric input

Initial proposed range:

- 10% through 95%;
- whole-percent increments.

The exact range can be revisited before implementation if backend semantics justify a different range.

Slider and numeric input represent the same pending value and must remain synchronized.

Examples:

- dragging the slider to 55 updates the input to 55 and enables Set when 55 differs from the active value;
- typing 47 moves the slider to 47 and enables Set;
- invalid input disables Set and shows a concise validation state.

Invalid examples include blank, non-numeric, and out-of-range values.

## Set button

`Set` represents a valid unapplied change.

Rules:

- disabled when pending value equals current configured value;
- enabled when pending value is valid and differs from configured value;
- disabled for invalid input;
- after successful application, the new value becomes current and Set returns to disabled.

## Configuration persistence

The GUI must read and write the existing user-editable TOML file rather than creating a second settings database.

Linux:

- `$XDG_CONFIG_HOME/poe2-auto-flask/config.toml`;
- fallback `~/.config/poe2-auto-flask/config.toml`.

Windows:

- `%APPDATA%\poe2-auto-flask\config.toml`.

Manual edits must remain supported after the GUI exists.

If the config changes externally while the GUI is open, the UI should update after the backend reloads it.

## Overview page

Overview is status-focused and should expose useful information without requiring `--debug`.

Candidate fields:

- connected/disconnected;
- automation state;
- HP current/max;
- Mana current/max;
- ES current/max;
- current area and area type;
- focus state;
- Life threshold and enabled state;
- Mana threshold and enabled state;
- detected Life and Mana bindings;
- Life and Mana flask current charges/cost where useful.

Do not expose raw memory addresses or implementation-level pointer data in the normal UI.

## Bottom telemetry strip

Keep visible across the primary pages where practical.

Fields:

- HP current/max;
- Mana current/max;
- ES current/max;
- current area;
- application CPU usage;
- application memory usage;
- update state.

For multi-process builds, CPU/RAM should preferably represent the combined application footprint rather than only the UI process.

## Status page

Status is a compact troubleshooting view, not a full logging console.

Candidate fields:

- PoE2 connection;
- game focus;
- automation state;
- current area and area type;
- detected flask bindings;
- flask charges/cost;
- config loaded/reloaded state;
- backend running state.

A short recent-event list can be added later if it proves useful.

## Update UX

Initial behavior:

- lightweight check shortly after startup;
- cache the result for the session;
- manual `Check now` action;
- failure to check must never affect automation.

States:

- `Up to date`;
- `Update available`;
- `Could not check for updates` with retry action.

First GUI implementation should use `View release` rather than attempting self-update. Automatic update installation is deferred.

## Behavior without PoE2

The future GUI should be able to remain running while PoE2 is closed.

Disconnected state should allow configuration editing and display muted telemetry placeholders such as `-`.

Expected model:

1. poe2-auto-flask starts;
2. if PoE2 is absent, it waits quietly;
3. when PoE2 appears, the backend connects automatically;
4. the overlay can be summoned once the application is running.

This differs from the current terminal-oriented AppImage lifecycle and will require an architecture change.

## System tray

Not mandatory for the first visual prototype, but likely useful long term.

Potential tray actions:

- Open overlay;
- Arm / Disarm;
- Open config;
- Check for updates;
- Quit.

## Performance goals

The UI should be intentionally light.

Targets:

- overlay hidden: near-zero GPU activity and very low CPU activity;
- overlay open but idle: well below 1% CPU on normal desktop hardware where practical;
- transitions: smooth at the display refresh rate where practical;
- UI rendering must never interfere with backend polling or flask decisions.

Suggested UI update cadence:

- HP/Mana/ES: roughly 100-250 ms;
- automation state: event-driven/immediate;
- area: change-driven;
- bindings: change-driven or approximately every 2 seconds;
- config: current backend live-reload behavior;
- CPU/RAM: approximately 1 second;
- update check: startup/manual only.

## UI/backend separation

The GUI must not take over game-memory reading or flask decision logic.

Backend responsibilities:

- PoE2 connection;
- memory reads;
- vitals/area/flask data;
- flask decision logic;
- input;
- config reload;
- authoritative automation state.

UI responsibilities:

- presentation;
- overlay/window interaction;
- configuration editing;
- update presentation;
- user commands sent to the backend.

Use a small IPC/event protocol between UI and backend.

## First implementation scope

First interactive prototype should contain:

- overlay shell;
- darkened background;
- fast slide/fade open and close;
- overlay hotkey;
- selected visual theme;
- Life and Mana toggles;
- Life and Mana slider/input/Set validation;
- hideout toggle;
- connected state;
- automation state;
- HP/Mana/ES;
- area;
- CPU/RAM;
- config read/write and external reload reflection;
- current version/update-available indicator with View Release.

## Deferred features

Do not include in the first implementation unless needed to unblock architecture:

- automatic updater;
- multiple themes;
- custom accent colors;
- draggable overlay placement;
- multiple overlay layouts;
- charts/history;
- large log viewer;
- per-character profiles;
- complex notification system;
- advanced animation settings.

## Visual baseline

Treat the selected mockup from the design discussion as the first-pass reference direction:

- top-centered overlay;
- dark translucent styling;
- fast slide-down transition;
- large side-by-side Life/Mana cards;
- Discord-like toggles;
- slider plus precise numeric input plus Set;
- clear connection/automation state;
- compact telemetry strip;
- subtle update notice;
- rounded, professional appearance.

Do not redesign the product into a traditional desktop settings window unless a platform limitation requires a fallback mode.
