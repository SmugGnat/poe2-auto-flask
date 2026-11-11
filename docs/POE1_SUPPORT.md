# PoE1 support — consolidated development notes

Canonical development branch: `feature/poe1-support`. This branch incorporates the read-only session work, automated PoE1 flask support, all 18 standard utility base types, and the optional movement-gated Quicksilver rule. Its starting code matches the last live-tested build from `feature/poe1-utility-movement`, with a single squashed commit on top of `develop`.

## Scope and architecture

- PoE1 Steam AppID **238960**; PoE2 Steam AppID **2694490**. The Linux launcher attaches to the selected Steam process and uses its active Proton prefix through `proton runinprefix`.
- The shared read-only `GameSession` owns one current-tick snapshot, pointer resolution, read caching, and refreshes on transitions. The PoE1 sampler requests vitals, area, flask inventory, effects, chat, and animation only as needed. The PoE2 automation loop retains its established charge/buff readers and rules.
- The PoE1 memory reader checks the Game States signature, in-game state, local-player identity, and character metadata. It reads usable (unreserved) Life/Mana, ES, the five equipped flask paths and charges, player effects, chat state and optionally Actor animation.
- Starting milestone 1 was **diagnostics-only**. The same backend now powers opt-in automated normal key input. Memory access remains read-only, without injection, kernel drivers or memory writes.

Selected offsets transcribed from a GameHelper C# snapshot (comments for the 2026-09-05 PoE1 executable):

| Structure | Offset |
| --- | --- |
| InGameState → AreaInstance | 0x218 |
| InGameState → WorldData | 0x270 |
| InGameState → in-game UI | 0x978 |
| AreaInstance → LocalPlayerStruct | 0x968; LocalPlayer +0x08 |
| Life component → Life / Mana / ES | 0x178 / 0x1C8 / 0x210 |
| ServerData → player inventories | 0xC9F8 |
| Inventory → equipped item vector | 0x170 |
| Buffs → effects vector | 0x160 |
| Actor → animation ID | 0x248, Run = 4 |

**Provenance:** The originally supplied GameHelper source snapshot had no verified original repository commit, upstream URL, or license. To avoid redistributing unverified third-party source, the consolidated branch **does not include** its approximately 3.6 MB C# reference tree. This project does not compile or link GameHelper. Relevant observations are independently implemented in Rust and documented here. Verify provenance and any attribution/licensing obligations before redistributing that original C# source.

## Configuration

The existing `~/.config/poe2-auto-flask/config.toml` is shared and backwards-compatible. PoE1 Life and Mana use the same threshold percentage settings (calculated using the unreserved available pool). For example:

```toml
[health]
enabled = true
threshold_percent = 50.0

[mana]
enabled = true
threshold_percent = 25.0

[poe1]
# Optional continuous Run animation required for Quicksilver;
# 0 = off, otherwise 100–5000 milliseconds
quicksilver_min_run_ms = 750

[poe1.utility]
quicksilver = true
jade = false
silver = false
```

All optional utility settings default **off**, and the movement requirement defaults to **0**. The full set of 18 utility types is: Diamond, Ruby, Sapphire, Topaz, Granite, Quicksilver, Amethyst, Quartz, Jade, Basalt, Aquamarine, Stibnite, Sulphur, Silver, Bismuth, Gold, Corundum and Iron. Unknown utility bases and Hybrid recovery flasks remain manual-only. Future UI management can list only the equipped types; the TOML remains explicit without any required slot/key fields.

PoE1 flask keys are read from `Documents/My Games/Path of Exile/production_Config.ini`, using all five `use_flask_in_slotN` entries in the selected input section. Keybinding availability is checked again on file changes. Invalid, ambiguous, missing, controller, mouse-button and duplicate bindings prevent input.

## Decisions, duplicate flasks and charge modifiers

- The engine classifies by the base-item path, not by slot or charges. It detects the type of the currently equipped item, so moving Quicksilver to another slot requires no config change.
- Life and Mana actions take priority over utility flasks. A resource action is eligible below its configured threshold. In a Chaos Inoculation-like configuration, an equipped Life flask is not used automatically.
- A duplicate-type selection uses maximum **whole remaining uses** (current charges divided by effective per-use cost), then greater charges, then lowest slot. If another flask of the same type is active, the engine avoids activating a second. This initial selection policy is intentionally simple.
- A flask's raw `Charges.PerUseCharge` may differ from actual charge cost. The reader uses the player's `flask_charges_used_positive_percentage` stat (ID `0x2E5`) and applicable explicit flask charge-use modifiers to calculate an effective, truncated integer cost. Unknown cost data blocks the affected flask rather than guessing.
- Special Life/Mana recovery effects such as **instant recovery on low life**, partial instant recovery, enchantment auto-use, and optimal flask rotation are **not yet modeled**. Their behavior may need verified modifier parsing and separate preferences.

## Optional Quicksilver movement gate

GameHelper's default Quicksilver rule used `PlayerAnimation.Equals(Animation.Run)` with a `Wait(1)` condition. This implementation samples `Actor.AnimationId` at `Actor+0x248` only if the option is on. Run = 4. It requires uninterrupted observed Run animation for the configured duration. Idle/cast animations, unrecognized animation, stale sample gaps, loss of focus, zone changes, disarming and invalid state reset the timer; the animation is checked again immediately before an input.

The gate only affects Quicksilver; it never prevents Life, Mana or another utility flask from being considered. Continuous movement is more reliable than assuming held left-click means actual character motion. Stutter-step casting builds may need a lower duration (e.g. 250 ms) or may leave the gate disabled.

## Safety, validation, and limitations

- Starts **DISARMED**; F11 only arms/disarms while the selected game is focused.
- Game state, focus, town/hideout permission, chat/UI visibility, death status, grace period, charges, active effects, modifier validity and a **350 ms per-slot input settling guard** gate input. This is not a flask cooldown. A Quicksilver use never delays a Life/Mana response. Invalid observations fail closed. The launcher does not invent a missing Proton prefix.
- `--game poe1 --debug` reads state only. It reports Life/Mana/ES, area, chat open/closed, base flask type, charges, raw/effective charge cost, active effects and Actor movement.
- **Live-tested successfully on a user's Steam Proton setup:** Life/Mana/ES and unreserved totals, area changes, five flask slots, matching player buffs, active Utility Flask effects, 26%-reduced Life flask cost (raw 15 → effective 11), chat detection, automatic PoE1 keybindings, Quicksilver and Life automation, F11 arming, focus and town/hideout protection, config reload, and optional ~750 ms Run gate.
- **Still needing broader live testing:** all 18 utility types, duplicate Life/Mana/utility selection, unusual/unique flask modifier behavior, gamepad input, other movement skills, and PoE2 end-to-end regression. Automated CI tests cannot prove future game versions preserve memory offsets.
- The reference C# snapshot is deliberately excluded from this feature branch and from distributions. Any future inclusion requires licensing/provenance verification.

## Experimental read-only nearby-monster census

GameHelper's reference `NearbyMonsterInfo` uses the already-maintained awake entity collection to count monsters by Normal/Magic/Rare/Unique rarity in two distance circles. This app does **not** maintain a game-wide entity collection, so a full entity tracker would be an unnecessary baseline cost. Its new experimental census runs **only** with an explicit diagnostic flag:

```fish
./poe2-auto-flask-linux-x86_64.AppImage --game poe1 --debug --nearby
```

The `--nearby` flag is **diagnostic-only**: it doesn't enable any flask use, doesn't add a nearby-combat setting yet, and cannot be used with `--auto`. The normal PoE1 and PoE2 automation loops do not call the census or pay its memory-scanning cost.

When enabled, the reader traverses only the PoE1 **awake** entity map (not sleeping or world decorations), with a hard limit of **1,500 map nodes** and **512 nearby monsters**. It caches static entity type/component addresses for the current area, limits remote rescans to once per **750 ms**, and groups the surviving monsters by rarity and proximity. It excludes dead/friendly/untargetable entities and known noncombat categories, including Harbingers and Tormented Spirits. It shows counts in **60** and **100 grid units** (same coordinate convention as GameHelper's circle distances). It fails closed on malformed/changing maps or invalid state; no partial result authorizes automation.

**Live validation (2026-11-11):** Normal, Magic, and Rare proximity counts were consistent with gameplay and friendly entities were excluded. During Kitava's Act 10 boss encounter, one Unique was counted before becoming vulnerable and the heart then appeared as a second Unique while still immune to damage. This is consistent with PoE's distinct attackable phases (Kitava and exposed Heart) and demonstrates that the current life/targetable flags are **not sufficient to prove an enemy can currently be damaged**. This is not fixed by pretending Unique always means combat: the reader remains diagnostic-only. Before enabling proximity-gated flasks, treat Unique-only rules conservatively and consider specific noncombat/invulnerable state checks (especially phased bosses). <https://www.poewiki.net/wiki/Kitava,_the_Insatiable>

**Performance instrumentation:** Normal PoE1 `--debug` and `--debug --nearby` now both report `Helper CPU: X% of one core (Y ms CPU / 5.0s wall)` at roughly five-second intervals, measured with Windows `GetProcessTimes` inside the helper under Proton. `--nearby` additionally prints `Nearby last full scan: Z ms wall` beside the rarity counts. This wall time includes cross-process read latency and **is not the scanner's CPU time**. The CPU percentage covers the helper's entire process (including logs and all readers), not PoE or the Wine server process. It is relative to one logical core, not system-wide Task Manager percentage; compare the two modes at similar monster density and area. Metrics are diagnostic-only and cannot trigger a flask. Under unsupported Wine configurations the CPU measure explicitly prints unavailable.

**Important:** This remains an experimental candidate parser. Boss-specific targetability and dense-map traversal need further validation. The scanner only executes in `--debug --nearby`; default PoE1/PoE2 automation performs no monster scan. A future per-type optional rule can combine minimum count, radius and rarity mask while reusing one shared census for all enabled utility flasks.

## Optional monster-proximity rules for utility flasks

The live-tested diagnostic scan added only approximately **0.1–0.2 percentage points of one CPU core** in a moderate density instance (193 awake nodes, 44 monsters; most recent full scan 7.29 ms wall). That is an observed measurement, not a guaranteed cost for more crowded maps or every machine.

GameHelper's reference `NearbyMonsterInfo` uses a rarity bitmask, with `Any` (all rarities) and `AtLeastRare` (Rare or Unique) presets. We expose a simpler **inclusive minimum rarity**: Normal matches Normal/Magic/Rare/Unique; Magic matches Magic/Rare/Unique; Rare matches Rare/Unique; and Unique matches Unique only. Thus a boss-only Unique is eligible with `minimum_rarity = "normal"` or `"rare"`, provided `minimum_count = 1`.

Existing `[poe1.utility]` switches enable each type as before. To make one type eligible only when monsters are nearby, optionally add a matching table:

```toml
[poe1.utility]
granite = true
jade = true

[poe1.nearby.granite]
radius = 60
minimum_count = 1
minimum_rarity = "normal"
```

Only Granite is proximity-gated here. Jade still uses the original effect-maintenance logic. A proximity table defaults to enabled, and can be suspended with `enabled = false` while retaining the other values. Valid radius is **1–150 grid units** and `minimum_count` is **1–512**. Unknown flask types/invalid values are rejected on config load; removing the table restores the previous behavior without changing the utility toggle.

The engine uses a single cached census for all proximity-enabled utility types, evaluated at most once per **750ms** while those rules are eligible. It does not scan with no enabled proximity conditions, while unarmed, out of focus, in a forbidden area, with chat open, or without a usable gated utility. Life/Mana are prioritized and evaluated **without waiting on monster scans**. If the census fails or exceeds its hard traversal bounds, only the proximity-dependent actions fail closed. The scanner isn't added to PoE2.

**Phased bosses:** A Unique monster can appear in the awake/targetable entity list before its encounter becomes damageable, as observed with Kitava and its Heart. This can cause a proximity-gated utility flask to trigger early. The current implementation deliberately doesn't claim to detect damageability or aggro. More precise boss state checks need validated data; we retain Unique because boss-only encounters would otherwise be excluded altogether.

**Count semantics:** A requirement of `minimum_count = 2` still requires **two** matching monsters. A single Unique does not automatically override that threshold. Use 1 for defensive utilities you want available against lone bosses.

## Build and test

Windows helper CI (Rust 1.85 MSRV): `cargo fmt --all -- --check`, `cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings`, `cargo test --locked --target x86_64-pc-windows-msvc`, and a release build.

Linux launcher CI: formatting, Clippy, unit tests, release build and AppImage packaging. The final successful multi-branch development build before consolidation was GitHub Actions run **42961739151** (56 Windows helper tests and 7 Linux launcher tests passed).

## Development workflow

Keep **one** active feature branch, `feature/poe1-support`. Commit subsequent fixes and enhancements to it; no new per-milestone branches or draft PRs. Preserve `main`, `develop`, and local uncommitted GUI work until explicitly asked to merge.
