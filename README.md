# PoE2 Auto Flask

> A lightweight Path of Exile 2 helper that automatically uses Life and Mana flasks based on configurable resource thresholds.

`poe2-auto-flask` runs as a small Windows x64 companion process. It works directly on Windows and can also run inside the same Steam Proton prefix as Path of Exile 2 on Linux.

The helper reads PoE2 state through ordinary **read-only** process APIs, checks whether a flask should be used, then sends the game's own configured flask key through `SendInput`.

---

## ✨ Features

- Automatic **Life Flask** and **Mana Flask** use.
- Configurable Life/Mana percentage thresholds.
- Reads PoE2's existing flask key bindings automatically — no duplicate key setup.
- Supports both **WASD** and traditional **mouse + keyboard** input modes.
- Supports normal keyboard bindings plus a single **Shift**, **Ctrl**, or **Alt** modifier.
- Detects live binding changes without restarting the helper.
- Reads real flask charges and will not attempt to use an empty flask.
- Avoids repeated presses while the corresponding flask effect is still active.
- Pauses automatically in towns, while dead, while PoE2 is unfocused, or when required state is unavailable.
- Hideout use is disabled by default and can be enabled in the config.
- Starts **DISARMED** every time; press **F11** while PoE2 is focused to arm/disarm.
- No installer, service, driver, administrator/root access, or game-memory writes.

---

## 🖥️ Compatibility

| Environment | Status |
| --- | --- |
| Steam Windows client through Proton/Wine | ✅ Runtime tested |
| Native Windows x64 | 🟡 Supported by the same Win32 code path; less field-tested |
| WASD input mode | ✅ Supported |
| Mouse + keyboard input mode | ✅ Supported |
| Gamepad/controller input mode | ❌ Detected, but auto-flask input is not supported yet |
| Keyboard flask bindings | ✅ Supported |
| Shift / Ctrl / Alt + keyboard key | ✅ Supported |
| Mouse-button flask bindings | ❌ Not supported yet |

PoE2 currently exposes two flask slots to the helper:

| Slot | Purpose |
| --- | --- |
| 1 | Life Flask |
| 2 | Mana Flask |

---

## 📦 Download and setup

Official binaries are published on the [GitHub Releases](https://github.com/SmugGnat/poe2-auto-flask/releases) page.

Each versioned release publishes the Windows x64 executable directly, plus its SHA-256 checksum and third-party dependency notices:

```text
poe2-auto-flask.exe
poe2-auto-flask.exe.sha256
THIRD_PARTY_NOTICES.txt
```

Official release binaries are built and published only by GitHub Actions from an exact version tag. The release workflow verifies that the tagged commit is contained in `main`, checks that the tag version matches `Cargo.toml`, reruns formatting/Clippy/tests, builds with the checked-in lockfile, and calculates the published SHA-256.

There is no installer or required ZIP. Put `poe2-auto-flask.exe` in a writable folder and start it once. If `config.toml` does not already exist beside the executable, the helper creates it automatically using the default settings. Existing `config.toml` files are never overwritten at startup.

`config.example.toml` remains in the source repository as a reference and test fixture; it is not required beside the release executable.

---

## ⚙️ Configuration

The default `config.toml` is:

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

### Settings

| Setting | Description |
| --- | --- |
| `enable_in_hideout` | Allows auto-flask use in hideouts. Towns are always blocked. |
| `health.enabled` | Enables/disables Life Flask automation. |
| `health.threshold_percent` | Use the Life Flask at or below this Life percentage. |
| `mana.enabled` | Enables/disables Mana Flask automation. |
| `mana.threshold_percent` | Use the Mana Flask at or below this Mana percentage. |

Thresholds must be between `1` and `99`.

The helper checks `config.toml` for changes while running, so threshold and enable/disable changes take effect without restarting it. Invalid live edits are rejected and the last valid settings remain active.

If a first-run config file cannot be created, the helper reports the error and continues with built-in defaults for that session.

Older `key = ...` and `poll_interval_ms = ...` entries are no longer used and can be removed. They are ignored if left in an older config.

---

## ⌨️ Automatic flask key bindings

There is **no flask-key setting** in `config.toml`.

The helper reads PoE2's own production config:

```text
Documents\My Games\Path of Exile 2\poe2_production_Config.ini
```

It automatically selects the correct binding set for the active input mode:

```text
mouse_and_keyboard -> [ACTION_KEYS]
wasd               -> [WASD_ACTION_KEYS]
gamepad            -> unsupported / no automated input
```

The helper then reads PoE2's configured bindings for:

```text
use_flask_in_slot1 -> Life Flask
use_flask_in_slot2 -> Mana Flask
```

Normal keyboard keys are supported, including a single Shift/Ctrl/Alt modifier such as:

```text
Q
Shift+Q
Ctrl+Q
Alt+Q
```

PoE2's config is checked for changes every couple of seconds using a lightweight file metadata check. The full file is only scanned after it actually changes, and the scanner uses setting names rather than fixed line numbers.

If a flask key is changed in PoE2, the helper picks it up automatically without a restart.

---

## ▶️ Running on Windows

Run:

```text
poe2-auto-flask.exe
```

or explicitly:

```text
poe2-auto-flask.exe --auto
```

The helper can be started before or after Path of Exile 2 on native Windows.

Once connected:

```text
F11     Arm / disarm auto-flask while PoE2 is focused
Ctrl+C  Exit from the console
```

No Rust installation, administrator rights, driver, or installer is required.

---

## 🐧 Running on Linux with Steam Proton

The helper itself is a Windows executable, so it should be launched inside PoE2's Proton prefix.

**Start Path of Exile 2 through Steam first.** Then launch the helper with `protontricks-launch`:

```fish
protontricks-launch --appid 2694490 \
    "$HOME/path/to/poe2-auto-flask.exe"
```

Replace the path with the actual location of the executable.

PoE2 does not need to be fully in-game yet; the helper can wait through login, character selection, and loading as long as the real game process already exists.

Under Proton/Wine, starting the helper before PoE2 can keep Steam's App ID occupied and prevent the game from launching. For that reason, if PoE2 is not already running the helper prints a start-order message and exits.

If PoE2 later exits or crashes, the helper exits as well. Restart PoE2 first, then launch the helper again.

---

## 🧠 How auto-flask decisions work

The helper continuously evaluates a small set of live game state while armed:

```text
Life / Mana
    + current area
    + flask charges
    + active flask state
    + PoE2's configured flask binding
            ↓
      threshold reached?
            ↓
   all safety gates valid?
            ↓
     SendInput key press
```

A flask press only happens when the relevant conditions are valid. In normal use this means PoE2 must be focused, the player must be alive, the current area must allow automation, the configured threshold must be reached, the flask must have enough charges, and the corresponding flask state must not already be active.

The helper also conservatively suppresses Life Flask automation for an obvious Chaos Inoculation-style state with one total Life and a real Energy Shield pool.

---

## ⏸️ When the helper pauses

The helper intentionally stops automated input when any required condition is not valid.

Common status messages include:

```text
Status: DISARMED
Status: ARMED
Status: PAUSED (town)
Status: PAUSED (hideout)
Status: PAUSED (PoE2 not focused)
Status: PAUSED (dead)
Status: PAUSED (gamepad input not supported yet)
Status: PAUSED (PoE2 input bindings unavailable)
Status: WAITING FOR GAME STATE
```

If a game update temporarily breaks a required memory structure or PoE2's binding config cannot be read safely, the helper stops sending input rather than guessing.

---

## 🔧 Debug mode

A read-only debug mode is available for troubleshooting:

### Windows

```text
poe2-auto-flask.exe --debug
```

### Proton

```fish
protontricks-launch --appid 2694490 \
    "$HOME/path/to/poe2-auto-flask.exe" --debug
```

Debug mode does **not** send flask input. It prints the currently resolved game state, area, vitals, flask state, and charge information.

---

## 🛠️ Building from source

The project targets Windows x64 and uses Rust 2021 with a minimum supported Rust version of 1.85.

```text
rustup target add x86_64-pc-windows-msvc
cargo fmt --check
cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc
```

The release executable is written to:

```text
target\x86_64-pc-windows-msvc\release\poe2-auto-flask.exe
```

Locally built binaries are useful for development, but official downloadable releases are produced only by the tag-driven CI release workflow.

---

## 📄 License and acknowledgements

This project is licensed under the **GNU General Public License v3 only** (`GPL-3.0-only`). See [`LICENSE`](LICENSE).

Path of Exile 2 reverse-engineering and memory-layout research was informed by existing community projects. See [`ACKNOWLEDGEMENTS.md`](ACKNOWLEDGEMENTS.md) for provenance and credits. Third-party Rust dependency terms are included with official releases in `THIRD_PARTY_NOTICES.txt`.

---

## 🔒 Process behavior

The helper opens the PoE2 process with only:

```text
PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION
```

It does **not** write game memory, inject a DLL, install a driver/service, require administrator/root privileges, or modify Linux `ptrace_scope`.

Automated actions are ordinary Win32 `SendInput` key events using the flask binding already configured inside PoE2.

---

## ⚠️ Disclaimer

This is an unofficial third-party automation tool. Game updates can change internal structures and may require an updated build. Users are responsible for deciding whether its use is appropriate under the game's current rules and terms.
