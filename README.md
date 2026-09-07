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

## 📄 License

Licensed under the **GNU General Public License v3.0 only** (`GPL-3.0-only`). See [`LICENSE`](LICENSE).

See [`ACKNOWLEDGEMENTS.md`](ACKNOWLEDGEMENTS.md) for upstream research references and credits.

---

## ⚠️ Disclaimer

This is an unofficial third-party tool. Path of Exile 2 updates may change compatibility. Use is at the user's discretion.
