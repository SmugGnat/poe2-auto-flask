# PoE Auto Flask

Lightweight flask automation for **Path of Exile 1 and Path of Exile 2** on
**Windows x64** and **Linux through Steam Proton**.

The application combines a Rust/Slint settings UI with the existing automation
backend in one Windows executable. It reads validated game state without
writing to game memory and sends ordinary key presses using the game's
bindings. No installer, administrator/root access, driver or DLL injection is
required.

## Features

- Automatic Life and Mana flasks with configurable thresholds in both games.
- PoE1: five flask slots, 18 opt-in utility types, Writhing Jar, duplicate
  rotation, charge-cost checks and optional movement/proximity conditions.
- PoE1: opt-in Bladefall of Trarthus, Blood Rage, Convocation, Immortal Call,
  Molten Shell, Spirit Offering and Steelskin presets.
- Profiles, validated settings editing, Save/Discard and manual TOML editing.
- Sidebar connection/automation state, Life/Mana/Shield, area and PoE1 name.
- Tray support; presentation work stops while hidden or minimized and
  automation continues independently.
- Automatic binding/configuration reload and fail-closed input guards.

Automation starts **disabled**. Press **F11** in the selected game to enable
it; change the shortcut in **Settings → Enable / Disable Key**. Automation
pauses when the game loses focus, in towns, while dead or when required game
state cannot be validated. Hideouts are blocked unless enabled in Settings.

## Version 1.0.3

PoE1 Nearby Enemies conditions now exclude hidden Unique enemies, bosses
frozen during Covens Fate selection, and more encounter objects that cannot
be fought. Enemies count again when the applicable effect clears. Read-only
boss diagnostics report changes to Life, effects, and encounter state.

## Download and run

Public assets on the [Releases page](https://github.com/SmugGnat/poe2-auto-flask/releases):

- Windows: `poe2-auto-flask-windows-x64.exe`
- Linux: `poe2-auto-flask-x86_64.AppImage`
- License notices: `LICENSES.txt`

These instructions describe this source tree's combined UI application;
older published releases may use the console interface. Development builds
are available as artifacts of the **Build** workflow on `develop`.

### Windows

Open the executable. It starts the UI and backend without a console, waits
for the game and reconnects after a game exit. Closing hides to the tray by
default; **Quit** stops the application. Starting another copy with the same
configuration directory restores the existing window.

Without `--game`, the application uses Steam context when present, otherwise
the unique client's install directory (`Path of Exile` or `Path of Exile 2`).
An unfamiliar directory prompts for the game in the UI. Multiple matching
clients are rejected. To select explicitly:

```text
poe2-auto-flask-windows-x64.exe --game poe1
```

### Linux / Steam Proton

Start the game through Steam, then make the AppImage executable once and run it:

```sh
chmod +x poe2-auto-flask-x86_64.AppImage
./poe2-auto-flask-x86_64.AppImage
```

The launcher discovers the running game's Steam AppID, active prefix and
selected Proton. It launches the combined Windows app with **`proton
runinprefix`**, without opening a terminal or installing files in the game
prefix. If both games are running, select with `--game poe1` or `--game poe2`.
The application exits with the game under Proton to preserve Steam cleanup.
No Protontricks, Steam Launch Options or `--auto` flag is required.

## Configuration

Use the UI or edit the same user-readable TOML files directly:

| Platform | Directory |
| --- | --- |
| Windows | `%APPDATA%\poe2-auto-flask\` |
| Linux | `$XDG_CONFIG_HOME/poe2-auto-flask/` or `~/.config/poe2-auto-flask/` |

```text
poe2-auto-flask/
  settings.toml          # active profiles, shortcut, tray and window preferences
  profiles/
    poe1-default.toml
    poe2-default.toml
```

An existing `config.toml` is validated and migrated without modification or
deletion. After migration it is a legacy copy; live settings are in
`settings.toml` and `profiles/*.toml`. Existing `window-state.toml` placement
is moved into `settings.toml` after a successful save.

Life/Mana thresholds accept 1–99% of the unreserved pool. Their rules default
to enabled at 60% Life and 30% Mana, while the application-wide automation
state still starts disabled. Utility flasks, unique flasks and skills default
to off. Flask bindings are read from the selected game's own configuration.
Profile switches and shortcut changes disable automation until the next
foreground shortcut press.

See [Profiles](docs/PROFILES.md) for the schema, migration and conflict handling,
and [PoE1 support](docs/POE1_SUPPORT.md) for optional rules and limitations.
[`config.example.toml`](config.example.toml) documents automation fields and
can also serve as a legacy migration source; it is not a profile document.

## Diagnostics and limitations

Read-only diagnostics open a console and send no automation input:

```text
poe2-auto-flask-windows-x64.exe --debug --game poe1
```

```sh
./poe2-auto-flask-x86_64.AppImage --debug --game poe1
```

PoE1 `--debug --nearby` adds the bounded nearby-monster/corpse census and
read-only Unique-boss details at a 250 ms target cadence. Timestamped snapshots
include identity, Life percentage, targetable/hidden fields, named boss states,
buffs, modifiers and numeric stats; later reports show changes.
**Ctrl+C** exits diagnostics. A game update that invalidates required memory
structures causes the helper to pause rather than guess.

Keyboard flask bindings are supported. Controller/gamepad automation and
mouse-button flask bindings are unsupported. PoE2 supports its keyboard/mouse
and WASD input modes; PoE1-only utilities and skills are not offered for PoE2.
PoE2 character-name telemetry is deferred.

Under Proton/KWin, Alt+Tab can leave parts of the UI undrawn. Taskbar/tray
activation or minimizing and restoring redraws it correctly. The cause is
unconfirmed. See the [UI guide](app-ui/README.md) for window behavior.

## Build and development workflow

All three crates use **Rust 1.100.0**, selected by `rust-toolchain.toml`.
Native Windows builds require the x64 MSVC target and Visual Studio C++ build
tools. Run these checks from the repository root on Windows:

```text
cargo fmt --all -- --check
cargo fmt --manifest-path app-ui/Cargo.toml -- --check
cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo clippy --locked --manifest-path app-ui/Cargo.toml --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-msvc
cargo test --locked --manifest-path app-ui/Cargo.toml --target x86_64-pc-windows-msvc
cargo build --locked --release --manifest-path app-ui/Cargo.toml --target x86_64-pc-windows-msvc
```

The local executable is
`app-ui/target/x86_64-pc-windows-msvc/release/poe2-auto-flask-ui.exe`.
The root console executable remains available for development/diagnostics.
For the native Linux launcher:

```sh
cargo fmt --manifest-path linux-launcher/Cargo.toml -- --check
cargo clippy --locked --manifest-path linux-launcher/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path linux-launcher/Cargo.toml
cargo build --locked --release --manifest-path linux-launcher/Cargo.toml
python3 scripts/test-app-run.py
```

The **Build** workflow runs affected component checks and assembles Windows
and AppImage artifacts on `develop` and `main`. Documentation-only changes
do not trigger automatic builds. It can also be dispatched manually for a
full build. Build and Release share packaging scripts, so `develop` artifacts
exercise the release assembly path.
Test those artifacts before promoting `develop` to `main`.

A push to `main` runs Build; it does **not** publish a release. **Release**
runs only for a `vMAJOR.MINOR.PATCH` tag contained in `main`, or an explicit
manual version request. It checks the root/launcher versions and builds,
tests and publishes the three public assets. Do not create tags or dispatch
Release as part of development validation.

See [UI architecture and development](app-ui/README.md),
[Source layout](docs/SOURCE_LAYOUT.md) and [Profiles](docs/PROFILES.md).

## License

Licensed under **GPL-3.0-only**. See [LICENSE](LICENSE) and
[ACKNOWLEDGEMENTS.md](ACKNOWLEDGEMENTS.md). Packaged builds include project,
dependency and bundled-font terms in `LICENSES.txt`.

This is an unofficial third-party tool. Game updates can change compatibility;
use is at the user's discretion.
