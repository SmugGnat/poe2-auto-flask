# PoE2 Auto Flask

> Lightweight, configurable flask automation for **Path of Exile 1 and Path of Exile 2**.

Runs on **Windows x64** and **Linux through Steam Proton**.

The helper reads validated game state without writing to game memory and sends ordinary flask key presses using the selected game's configured bindings. No installer, administrator/root access, driver, or DLL injection is required.

**New in v0.8.0:** PoE1 support, a shared automation engine with independent game-specific policies, and automatic Steam/Proton game detection on Linux.

---

## ✨ Features

- Automatic **Life** and **Mana** flask use in both games with configurable thresholds
- **PoE1:** five equipped flask slots, automatic key discovery, and optional type-aware utility-flask rules for 18 base types
- **PoE1:** effective flask charge costs, duplicate-flask rotation, optional Quicksilver movement and bounded nearby-monster conditions
- **PoE2:** preserves its established two-slot Life/Mana logic and flask safety guards
- Detects game keybinding and configuration changes while running
- Checks flask charges and active effects; refuses input on invalid or stale game state
- Starts **DISARMED** every time; only F11 in the selected game's focused window arms automation

---

## 📦 Download

Download the file for your OS from the [Releases](https://github.com/SmugGnat/poe2-auto-flask/releases) page:

- **Windows x64:** `poe2-auto-flask-windows-x64.exe`
- **Linux x86-64:** `poe2-auto-flask-x86_64.AppImage`

There is no installer.

---

## ⚙️ Configuration

Default config:

```toml
[general]
enable_in_hideout = false

[health]
enabled = true
threshold_percent = 60.0

[mana]
enabled = true
threshold_percent = 30.0
```

| Setting | What it does |
| --- | --- |
| `enable_in_hideout` | Allows auto-flask use in hideouts. Towns are always blocked. |
| `health.enabled` | Enables Life Flask automation. |
| `health.threshold_percent` | Life percentage that triggers the Life Flask. |
| `mana.enabled` | Enables Mana Flask automation. |
| `mana.threshold_percent` | Mana percentage that triggers the Mana Flask. |

Thresholds can be set from `1` to `99`.

The config file is stored in the normal per-user config location:

**Windows**

```text
%APPDATA%\poe2-auto-flask\config.toml
```

**Linux**

```text
~/.config/poe2-auto-flask/config.toml
```

`$XDG_CONFIG_HOME` is used instead of `~/.config` when it is set.

Changes to `config.toml` are picked up automatically while the helper is running.

Flask keys do not need to be added to the config. The selected game's keyboard bindings are read automatically (PoE1 via `production_Config.ini`, PoE2 via its own configuration). PoE1 utility flasks are disabled by default and must be explicitly enabled under `[poe1.utility]`; see the [PoE1 support guide](docs/POE1_SUPPORT.md).

---

## ▶️ Running

### Windows

Select the game explicitly when using PoE1:

```text
poe2-auto-flask-windows-x64.exe --game poe1
```

Without `--game`, the Windows helper defaults to PoE2:

```text
poe2-auto-flask-windows-x64.exe
```

PoE2 can be started before or after the helper. PoE1 uses its game-specific reader and policy.

### Linux / Steam Proton

Make the AppImage executable once:

```bash
chmod +x poe2-auto-flask-x86_64.AppImage
```

Start PoE1 or PoE2 through **Steam** first, then double-click the AppImage or run:

```bash
./poe2-auto-flask-x86_64.AppImage
```

On Linux the launcher auto-selects the **one** running recognized Steam/Proton game by AppID and uses its active Proton prefix. If **both** games are running, choose explicitly with `--game poe1` or `--game poe2`; the launcher will not guess. If the selected game closes, its helper closes as well.

**No `--auto` flag is needed.** Both games default to normal automation, starting DISARMED. Use `--debug` for read-only diagnostics.

---

## ⌨️ Controls

```text
F11     Arm / disarm auto-flask
Ctrl+C  Exit
```

F11 changes the armed state only while the **selected game** is focused. The helper starts **DISARMED** every time.

While armed, automation pauses automatically in towns, when the selected game is not focused, when the player is dead, or when required game state cannot be read safely.

Hideouts are blocked by default and can be enabled in `config.toml`.

Keyboard flask bindings are supported; PoE2 supports its WASD and mouse + keyboard input modes. Controller/gamepad automation and mouse-button flask bindings are not currently supported.

---

## 🔧 Troubleshooting

Read-only debug mode can be started with:

**Windows**

```text
poe2-auto-flask-windows-x64.exe --debug
```

**Linux**

```bash
./poe2-auto-flask-x86_64.AppImage --debug
```

Debug mode shows the selected game's state, area, Life/Mana values, flask charges, and active effects without sending flask input. On Windows, add `--game poe1` to debug PoE1; on Linux, single-game auto-detection also applies in debug mode.

If a game update changes required memory structures, the helper pauses rather than guessing and sending unsafe input.

---

<details>
<summary><strong>Building from source</strong></summary>

The Windows helper requires Rust 1.85 or newer and the Windows x64 MSVC target.

```text
rustup target add x86_64-pc-windows-msvc
cargo fmt --check
cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc
```

The Linux launcher is a separate Rust crate under `linux-launcher/`.

```text
cd linux-launcher
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

Release AppImages are assembled by GitHub Actions.

</details>

---

## 🧪 PoE1-specific options and limitations

PoE1 support is included in **v0.8.0**. It provides a validated read-only reader, five-slot flask automation, type-aware discovery, charge-cost handling, automatic keybindings and optional Quicksilver movement gating. Normal Life/Mana rules are on by default; utility types require opt-in configuration.

Start PoE1 through Steam before launching the release AppImage. On Linux,
the launcher detects which game is running from its validated Steam AppID
and Proton prefix. For normal automation, just launch the AppImage with
**no flags**:

```bash
./poe2-auto-flask-x86_64.AppImage
```

Automation is the default for both games, but starts **DISARMED**; press **F11**
while the selected game is focused to arm. To inspect game state without
sending flask inputs, run:

```bash
./poe2-auto-flask-linux-x86_64.AppImage --debug
```

The optional `--auto` flag remains supported for existing commands but is
not required. If **both** PoE1 and PoE2 are running, choose explicitly with
`--game poe1` or `--game poe2`. The launcher never guesses between two
running clients. If neither is detected, PoE2 remains the fallback selection,
but an active Steam Proton environment is still required. On Windows,
automatic Steam detection is not used; PoE2 remains the default unless
`--game poe1` is specified. PoE1 uses `[health]` and `[mana]` thresholds, and individually opt-in utility types under `[poe1.utility]`. Duplicate equipped flasks are selected automatically with least-recently-used rotation and a separate same-resource guard for instant Life/Mana recovery. A Life-only emergency exception permits a positively identified instant Life flask to bypass an already-active gradual recovery effect at very low Life; conditional-instant flasks wait for actual Low Life (50% of maximum or less). All 18 standard PoE1 utility base types are recognized; a `quicksilver_min_run_ms` option under `[poe1]` optionally delays Quicksilver until sustained running. Keybindings are discovered from PoE1's own `production_Config.ini`.

The **[PoE1 support guide](docs/POE1_SUPPORT.md)** documents memory architecture, configuration, duplicate-flask choices, safety checks, validation, and known limitations.

A new **experimental, read-only** nearby-monster census is available with `--game poe1 --debug --nearby`. It reports rarity counts at 60 and 100 grid units, with strict scan-size/time limits; optional nearby conditions for utility flasks now share the cached reader when enabled. PoE1 debug mode now reports the helper's own **5-second CPU usage average**; with `--nearby` it also reports wall time spent on the last full entity scan, making baseline comparisons possible. A rarity threshold is inclusive (e.g. `rare` matches Rare and Unique); existing flask behavior remains unchanged unless you add a `[poe1.nearby.<type>]` table. Boss intro phases may still count before damage can be dealt. The external GameHelper C# reference tree is not bundled pending verification of its source license and provenance.

---


## 📄 License

Licensed under the **GNU General Public License v3.0 only** (`GPL-3.0-only`). See [`LICENSE`](LICENSE).

See [`ACKNOWLEDGEMENTS.md`](ACKNOWLEDGEMENTS.md) for upstream research references and credits.

---

## ⚠️ Disclaimer

This is an unofficial third-party tool. Path of Exile 1 or 2 updates may change compatibility. Use is at the user's discretion.
