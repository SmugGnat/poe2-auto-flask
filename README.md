# PoE2 Auto Flask

> A small Path of Exile 2 helper that automatically uses Life and Mana flasks when they fall below configurable thresholds.

Runs on **Windows x64** and **Linux through Steam Proton**.

The helper reads game state without writing to game memory and sends normal flask key presses using the bindings already configured in PoE2. No installer, administrator/root access, driver, or DLL injection is required.

---

## ✨ Features

- Automatic **Life Flask** and **Mana Flask** use
- Configurable Life and Mana thresholds
- Reads PoE2's flask key bindings automatically
- Detects binding and config changes while running
- Checks flask charges before using a flask
- Avoids repeated presses while a flask effect is active
- Pauses automatically when automation is not safe or allowed
- Starts **DISARMED** every time

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

Flask keys do not need to be added to the config. The helper reads them directly from PoE2 and also detects binding changes while running.

---

## ▶️ Running

### Windows

Run:

```text
poe2-auto-flask-windows-x64.exe
```

PoE2 can be started before or after the helper.

### Linux / Steam Proton

Make the AppImage executable once:

```bash
chmod +x poe2-auto-flask-x86_64.AppImage
```

Start Path of Exile 2 through Steam first, then double-click the AppImage or run:

```bash
./poe2-auto-flask-x86_64.AppImage
```

If PoE2 closes, the helper closes as well.

---

## ⌨️ Controls

```text
F11     Arm / disarm auto-flask
Ctrl+C  Exit
```

F11 only changes the armed state while PoE2 is focused. The helper starts **DISARMED** every time.

While armed, automation pauses automatically in towns, when PoE2 is not focused, when the player is dead, or when required game state cannot be read safely.

Hideouts are blocked by default and can be enabled in `config.toml`.

Keyboard flask bindings are supported in both WASD and mouse + keyboard modes. Controller/gamepad automation and mouse-button flask bindings are not currently supported.

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

Debug mode shows the detected game state, area, Life/Mana values, flask charges, and active flask state without sending flask input.

If a PoE2 update changes required game structures, the helper stops sending input rather than guessing.

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

## 🧪 PoE1 support (development branch)

PoE1 support is developed on **[`feature/poe1-support`](https://github.com/SmugGnat/poe2-auto-flask/tree/feature/poe1-support)**. It includes the validated read-only PoE1 memory reader, five-slot flask automation, type-aware flask discovery, modifier-adjusted charge costs, automatic keybinding detection, and an optional movement gate for Quicksilver. This functionality is **not yet part of the public release**.

Start PoE1 through Steam before launching a development AppImage. On Linux,
the launcher now detects which game is running from its validated Steam AppID
and Proton prefix, so an explicit `--game` is normally unnecessary:

```bash
./poe2-auto-flask-linux-x86_64.AppImage --debug
```

For PoE1 automation (starts **DISARMED**; press **F11** in the focused game to arm):

```bash
./poe2-auto-flask-linux-x86_64.AppImage --auto
```

If **both** PoE1 and PoE2 are running, choose explicitly with `--game poe1` or
`--game poe2`. The launcher never guesses between two running clients.
If neither is detected, PoE2 remains the fallback selection, but an active
Steam Proton environment is still required. PoE1 still requires an explicit
`--auto` or `--debug` mode even when detected automatically. On
Windows, automatic Steam detection is not used; PoE2 remains the default
unless `--game poe1` is specified. PoE1 uses `[health]` and `[mana]` thresholds, and individually opt-in utility types under `[poe1.utility]`. Duplicate equipped flasks are selected automatically with least-recently-used rotation and a separate same-resource guard for instant Life/Mana recovery. A Life-only emergency exception permits a positively identified instant Life flask to bypass an already-active gradual recovery effect at very low Life; conditional-instant flasks wait for actual Low Life (50% of maximum or less). All 18 standard PoE1 utility base types are recognized; a `quicksilver_min_run_ms` option under `[poe1]` optionally delays Quicksilver until sustained running. Keybindings are discovered from PoE1's own `production_Config.ini`.

The **[PoE1 support guide](docs/POE1_SUPPORT.md)** documents memory architecture, configuration, duplicate-flask choices, safety checks, validation, and known limitations.

A new **experimental, read-only** nearby-monster census is available with `--game poe1 --debug --nearby`. It reports rarity counts at 60 and 100 grid units, with strict scan-size/time limits; optional nearby conditions for utility flasks now share the cached reader when enabled. PoE1 debug mode now reports the helper's own **5-second CPU usage average**; with `--nearby` it also reports wall time spent on the last full entity scan, making baseline comparisons possible. A rarity threshold is inclusive (e.g. `rare` matches Rare and Unique); existing flask behavior remains unchanged unless you add a `[poe1.nearby.<type>]` table. Boss intro phases may still count before damage can be dealt. The external GameHelper C# reference tree is deliberately not bundled in this branch pending verification of its source license and provenance.

---


## 📄 License

Licensed under the **GNU General Public License v3.0 only** (`GPL-3.0-only`). See [`LICENSE`](LICENSE).

See [`ACKNOWLEDGEMENTS.md`](ACKNOWLEDGEMENTS.md) for upstream research references and credits.

---

## ⚠️ Disclaimer

This is an unofficial third-party tool. Path of Exile 2 updates may change compatibility. Use is at the user's discretion.
