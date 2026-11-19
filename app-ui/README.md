# PoE Auto Flask UI

This is the production Rust/Slint settings UI, compiled into the same Windows
executable as the automation backend. Linux packages that executable in an
AppImage and runs it through the selected game's active Steam Proton.
This file is the UI architecture and behavior reference.

## Runtime ownership

| Component | Responsibility |
| --- | --- |
| Slint main thread | Presentation, settings drafts, profile operations, window and tray |
| Automation thread | Existing game readers, input, safety gates and 100 ms decision loop |
| Status bridge thread | Coalesced backend status and demand-driven read-only telemetry |
| Instance listener | Waits for a second launch to request window restoration |
| Native Linux launcher | Discovers the live Proton environment and launches the Windows app |

`poe2-auto-flask` is a shared Rust library. `BackendHost` starts the backend
with bounded Rust channels (eight status snapshots and eight commands).
Statuses contain owned data, never game pointers or UI handles. Full status
queues retain the latest pending state for later delivery. The UI coalesces
notifications to avoid an event-loop backlog. There is no separate helper
process, IPC service, webview or native Linux frontend.

The backend remains authoritative for connection and automation. The UI
persists settings through the same `ProfileStore` and validators used by the
backend; its existing once-per-second configuration check applies changes.
UI telemetry never authorizes input or blocks the automation loop.

## Presentation and editing

Slint **1.19.0** is pinned alongside `slint-build`; all crates use the root
Rust **1.100.0** toolchain. The Linear-inspired UI uses opaque charcoal
surfaces, restrained blue accents and bundled Inter Variable. The same blue
icon is used by the sidebar, tray, Windows executable and AppImage.

- **Flasks:** Life/Mana recovery, PoE1 utility selection and Writhing Jar.
- **Skills:** PoE1 Molten Shell, Blood Rage and Spirit Offering presets.
- **Profiles:** creation at the top, active selection and Rename / Duplicate /
  Reset / Delete menus. Reset confirms inline and restores backend defaults.
  The last profile for each game is protected.
- **Settings:** profile-specific Enable in hideouts, plus the app-wide
  Enable / Disable Key and Close to system tray preferences.
- **Auto Flask menu:** Settings and Quit. The tray offers Open, Settings and Quit.

Enabled skill headers have a muted disclosure arrow. Clicking the name or
header space folds or unfolds options without disabling the skill or changing
its draft. The toggle stays independent; enabling a skill opens its options.
Fold state saves immediately under `[ui.skills]` in `settings.toml`, so it
survives restarts without changing the automation profile or its dirty state.

Profile changes appear in a **Save / Discard** footer on every page. Invalid
or unsaved drafts block profile switching. Selecting another utility first
captures the current utility's draft and then loads that utility's own state.
Unknown/unexposed profile fields survive saves. Stale profile revisions are
rejected rather than overwriting external edits. Creating a profile activates
it; duplicating another profile leaves the current selection unchanged.
Reset changes automation settings, preserving the profile's identity and
global preferences.

Numeric inputs filter invalid text and use the shared backend range checks
when saved. Skill fields record letters, digits or function keys with optional
Ctrl/Shift/Alt modifiers; numpad digits are normalized to digits. The app-wide
automation key is a single letter, digit or F1–F12, defaulting to F11.
Changing that key or the active profile disables automation until a fresh
press in the focused game.

Nearby sliders cover **1–150 grid units**, default to **60**, and gently snap
around 60 and 100. The hovered/dragged thumb grows and displays its value in a
tooltip. Selectors remain inside the window, scroll up to eight visible rows,
keep the current choice visible and support keyboard navigation. Only the
hovered/navigated choice is highlighted; the selected choice has a check.
Enabled flask/skill names use the relevant PoE colors. Animations are limited
to brief opening and interaction transitions.

## Sidebar data

The compact sidebar contains the quick profile selector, connection and
automation indicators, optional PoE1 name, Life/Mana/Shield and area.

- Game text reports connection health. Its tooltip describes connection/errors.
- The dot is green when automation is enabled and red when disabled. Its
  tooltip owns temporary focus/area pause reasons and shortcut information.
- Successful settings acknowledgements remain internal; errors are displayed
  when action is needed.
- Vitals use unreserved maxima and comma grouping, such as `4,364/4,364`.
  Verified absent PoE1 Shield displays `0/0`; unavailable data uses a dash.
- PoE1 character name is optional and has no tooltip. PoE2 names are deferred.

The bridge opens an independent read-only session through the validated game
adapters and samples at most **twice per second while visible**. Failed reads
clear telemetry, with slower retries after attach/read failures. The PoE1
name uses a bounded Player-component MSVC wide-string read at `0x168`, with
owner, player identity, descriptor and text validation. A name failure omits
only the name. No game-memory interpretation is implemented in `.slint`.

## Hidden/minimized lifecycle

`Presentation` owns the editor's two-second metadata/geometry timer. Profile
draft notifications update dirty state immediately, independently of that
timer. When hidden, minimized or fully covered by the selected fullscreen game:

- Stop editor refresh and telemetry reads.
- Cache backend status without scheduling Slint updates.
- Dismiss menus/tooltips and stop tooltip timers.
- Continue the independent automation thread and explicit shutdown handling.

Restore requests fresh backend data, reconciles external files and restarts
the editor timer without discarding unsaved drafts. A visible UI on another
monitor continues to display live data. There are no periodic hidden redraws
or presentation polling loops.

Normal launch is a Windows GUI program without a console. `--debug` attaches
a console (or allocates one if needed) and runs existing read-only diagnostics
instead of starting the UI. Explicit Quit requests backend shutdown and waits
for completion. Closing hides to the tray by default; that preference can be
disabled. Native Windows waits/reconnects; under Proton the app exits with
the game so Steam can release the session. One instance is allowed per
configuration directory; a second launch restores the existing window.

## Storage and window behavior

The shared profile root is `%APPDATA%\poe2-auto-flask\` on native Windows,
and `$XDG_CONFIG_HOME/poe2-auto-flask/` or `~/.config/poe2-auto-flask/` on
Linux. `--config-dir PATH` takes precedence over `POE2_AUTO_FLASK_PROFILE_DIR`.
The launcher maps the host root through Wine's Z: drive. Manual Proton
launches can discover inherited HOME/XDG paths; if unavailable, supply the
root explicitly rather than creating a second store inside the prefix.
See [Profiles](../docs/PROFILES.md) for schema and migration.

Global preferences and last normal window placement live in **settings.toml**:

```toml
[ui]
hide_to_tray_on_close = true

[ui.window]
x = 120
y = 160
width = 780
height = 552
```

Placement saves only when changed, using the profile store's atomic replacement
and external-edit protection. It is independent of profile Save/Discard and
does not disable automation. An existing `window-state.toml` is imported only
when no valid placement is already stored; it is removed after the settings
save succeeds. No new standalone window-state file is created.

The first window is centered on the available monitor. Saved placement is
clamped to a currently available monitor. Minimized/maximized geometry does
not replace normal geometry. Where positioning is unavailable, preserve saved
coordinates and update size only.

The default renderer is Winit software. `--renderer=femtovg` is available for
comparison; software avoids the uneven glyph rasterization seen under Proton.
The native window is opaque and borderless. Before its first show on Windows,
placement, rounded region and a dark native background-erasure handler are
prepared while the HWND is hidden. This avoids exposing the default backing
surface before Slint's first visible frame. Rounded regions do not request
a separate synchronous native repaint. Win32 regions provide rounded corners
under Proton without requesting a transparent alpha surface.
Activation invalidates the full client area once. A Wine-only stacking
workaround adjusts our UI beneath the backend-selected foreground fullscreen
game without activating either window; it does not change the game's window.

**Known issue:** returning through Alt+Tab under Proton/KWin can leave parts
of the UI undrawn. Taskbar/tray activation or minimizing and restoring redraws
correctly. The cause remains unconfirmed. The tray context menu is OS-supplied
and follows the platform's appearance.

## Source files

| File | Responsibility |
| --- | --- |
| `ui/main.slint` | Pages, shared controls, menus, tooltip state, tray |
| `src/main.rs` | Editor drafts, callbacks, persistence, presentation lifecycle |
| `src/backend_host.rs` | Automation worker lifetime, status bridge, telemetry formatting |
| `src/desktop.rs` | Console attachment, instance event/mutex, Win32 repaint/stacking |
| `build.rs` | Slint compilation, Windows icon and version resources |
| `../src/runtime.rs` | Owned backend snapshots and bounded command/status channels |
| `../src/telemetry.rs` | Independent presentation-only reader |
| `../src/profiles.rs` | Durable profiles, global preferences and atomic/conflict-safe saves |

## Build, packaging and validation

See the [root build instructions](../README.md#build-and-development-workflow)
for formatting, Clippy, both Windows test suites and the launcher checks.
Use the official Slint skill for `.slint` work; compile and inspect a render
after UI edits. Non-release builds retain compiler metadata for click-level
Slint regression tests. Release builds omit that test metadata.

The local Windows executable is
`app-ui/target/x86_64-pc-windows-msvc/release/poe2-auto-flask-ui.exe`.
Build and Release rename it to `poe2-auto-flask-windows-x64.exe`; the AppImage
bundles the same executable as `usr/lib/poe2-auto-flask/poe2-auto-flask.exe`.
The launcher maintains a host-side copy and uses `proton runinprefix`.
Desktop launch has `Terminal=false`; only explicit `--debug` opens a terminal.

All three lockfiles are versioned and workflows use `--locked`. The root Cargo
package supplies the Settings version and Windows file/product versions.
Packaging includes project/dependency licenses, Slint's license directories,
the Inter font license and version-specific supplements under
`../packaging/licenses/`.

Local validation includes Rust tests under isolated Proton, Linux launcher
and desktop launch-routing tests, release cross-builds and AppImage assembly.
User live PoE1/Proton testing confirmed profiles, settings, status/name data,
automation, tray/minimize behavior and the fullscreen click-through fix.
Broader native Windows/PoE2 checks are deferred. A preliminary user performance
check found automation comparable to the existing helper and negligible UI CPU;
the final full external performance review follows the `develop` CI build.
