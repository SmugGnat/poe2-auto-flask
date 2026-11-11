# PoE1 read-only Milestone 1

## Scope and branches

This experimental implementation is isolated on `feature/poe1-readonly-session`, based on `reference/gamehelper-poe1`. It does **not** update `main` or `develop` and does not depend on unfinished local UI changes.

- PoE2: existing auto-flask input, bindings, area rules, debug output and configuration stay intact. The automation loop obtains its vital readings through `GameSession`; the existing proven PoE2 charge/buff readers are intentionally unchanged.
- PoE1: explicitly selected read-only debug. `--auto --game poe1` is rejected before attachment. No key input, remote writes, injection or drivers are used.
- Linux: launcher detects the selected Steam AppID (PoE1 238960; PoE2 2694490) and still uses the active prefix with `proton runinprefix`.

## Design

`src/session.rs` is the sole coordinator of current-tick snapshots. `begin_tick` invalidates sampled data; subsequent calls to `vitals`, `poe1_area`, `poe1_flasks`, or `poe1_effects` collect only the requested data, once per tick. The Rust automation/rule layer never needs to read the game's internal offsets itself.

PoE1 address caches (flask item/Charges component and buff definition metadata) are separate from dynamic values (current charges, raw charge cost, time remaining). Cached addresses are invalidated when the session is refreshed or inventory pointers change. Errors do not silently reuse stale samples.

The PoE1 vital reader performs a single contiguous read for Life, Mana and Energy Shield after checking game state, area, local player and entity identity. Flask and effects data are only sampled by the diagnostics when requested. All vector sizes are bounded.

## GameHelper reference and constraints

Source files in `references/GameHelper/GameOffsets` supplied the **2026-09-05-executable** offsets and the 16-byte Game States signature. GameHelper's 5-slot inventory and effects logic also informed the Rust reader. The reference snapshot's upstream commit and original license file were not present in the imported directory; provenance remains to be verified before any direct source redistribution. This implementation does not link or compile the GameHelper C# code.

PoE1 offsets used in the first reader:

| Path | Offset |
| --- | --- |
| GameState → InGameState | 0x48 + index 4 × 0x10 |
| InGameState → AreaInstance | 0x218 |
| InGameState → WorldData | 0x270 |
| AreaInstance → PlayerInfo | 0x968 |
| PlayerInfo → LocalPlayer | +0x08 |
| Life → Health / Mana / ES | 0x178 / 0x1C8 / 0x210 |
| WorldData → area details → row | +0xA0, then +0xB0 |
| ServerData → player inventories | 0xC9F8 |
| Inventory item-list vector | 0x170 |
| Buffs → active-effect vector | 0x160 |

These are sourced from code, **not yet confirmed against a running PoE1 client**.

## Validation

### Automated checks

Root Windows helper:

```text
cargo fmt --all -- --check
cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo test --locked --target x86_64-pc-windows-msvc
cargo build --locked --target x86_64-pc-windows-msvc
```

Linux launcher:

```text
cd linux-launcher
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

The new unit tests cover game identity, invalidating snapshots, PoE1 vital layout and sanity checks, bounded memory vectors, active flask-effect attribution, inventory cache invalidation, and launcher Steam AppID selection. They cannot prove offsets are correct in a live game.

### Live PoE1 checks (still required)

1. Start PoE1 through Steam Proton and enter a character (not the login/character-selection screen).
2. Start the development launcher with `--game poe1 --debug` from a terminal.
3. Confirm reported Life, Mana, Energy Shield, current zone and five flask slots against the PoE1 UI.
4. Use a flask manually: confirm current charges decrement and active effects appear in the corresponding slot.
5. Change a flask-charges-used modifier, then compare the **raw** cost field with the observed charge delta. Do not assume that the raw cost is already modifier-adjusted or that GameHelper's calculation is required.
6. Replace and remove flasks, change zones, go to town/hideout, switch characters and close the client; the reader must update or fail closed.
7. Verify the original PoE2 `--auto` and `--debug` modes on both Windows and Linux, including F11, town, focus and gamepad safeguards.

While the PoE1 reader is unverified, diagnostics should be treated as development output only. **PoE1 automation is not available in this milestone.**

## Known limitations

- A single selected Path of Exile process is supported. The helper refuses to attach when more than one candidate process exists, rather than choosing an ambiguous client.
- No automatic PoE1 key-binding discovery or flask classification/decision rules yet (Milestone 2).
- Raw PoE1 charge-cost field is not confirmed to account for modifiers.
- The PoE1 status-effect parser and Game States pointer chain must be validated against a live game.
- This milestone does not claim any benchmarked CPU reduction.
