# PoE1 support — technical and validation notes

PoE1 support is part of the v0.8.0 multi-game release. The original work was developed on `feature/poe1-support`, reviewed and squash-merged into `develop` after automated and live testing. Historical development milestones and experiments are retained here as technical context; the release source of truth is `main` and its versioned tags.

## Scope and architecture

### Adapter separation — architecture milestone 1

The shared `GameSession` now dispatches to independent `poe1::adapter::Adapter` and
`poe2::adapter::Adapter` implementations of a small `GameStateReader` contract:
`refresh` and `read_vitals`. `game_state.rs` owns the common `VitalPool`,
`LiveVitals`, and per-tick `GameSnapshot` types; neither game is required
to use the other's raw offsets or in-process component layouts.

The PoE1 adapter owns the PoE1 player probe, charge/buff/motion/nearby caches
and PoE1-only area/flask/effect/animation snapshots. These are read only on
request and invalidated on each tick or session refresh as appropriate.
The shared snapshot caches **only common vitals**. Optional PoE1 capabilities
are accessed through explicitly game-specific session methods, which refuse
unsupported readers. The PoE2 adapter retains the proven PoE2 vitals probe;
its specialized charge and buff probes remain in the PoE2 policy pending
a later, independently verifiable adapter migration.

The adapters do not couple the games' raw structures. Step 3 consolidated
the lifecycle loop while preserving separate game policies. Linux game
selection now happens at launcher startup from validated Steam AppIDs
(Step 5), never in the shared engine. Game-specific state remains
demand-driven and fail-closed.

- PoE1 Steam AppID **238960**; PoE2 Steam AppID **2694490**. The Linux launcher auto-selects the one running recognized game, validates its active Proton prefix, and starts the matching helper policy through `proton runinprefix`. Explicit `--game` overrides remain available.
- Native Windows, without `--game`, identifies a unique running PoE process by its executable's **immediate parent directory** (`Path of Exile` or `Path of Exile 2`), ignoring case. Unknown directories require an interactive `1` (PoE1) / `2` (PoE2) prompt; no dual-parser probe occurs. Selection stays fixed for that process and the existing game-specific GameState validation still gates automation. Multiple matching clients or a missing interactive console fail closed; explicit `--game` bypasses detection. Wine/Proton continues using the launcher-selected game.
- The shared read-only `GameSession` owns one current-tick snapshot, pointer resolution, read caching, and refreshes on transitions. The PoE1 sampler requests vitals, area, flask inventory, effects, chat, and animation only as needed. The PoE2 policy retains its established charge/buff readers and rules.
- The PoE1 memory reader checks the Game States signature, in-game state, local-player identity, and character metadata. It reads usable (unreserved) Life/Mana, ES, the five equipped flask paths and charges, player effects, chat state and optionally Actor animation.
- Starting milestone 1 was **diagnostics-only**. The same backend now powers opt-in automated normal key input. Memory access remains read-only, without injection, kernel drivers or memory writes.

Selected **PoE1 3.30 candidate** offsets extracted from the supplied updated GameHelper `GameOffsets.dll` (2026-11-13). The archived C# reference is on branch `reference/gamehelper-poe1`, under `references/GameHelper/`. Values are metadata-derived and **not yet live-validated**:

| Structure | Offset |
| --- | --- |
| GameState → current state vector / states buffer | 0x10 / 0x50 |
| InGameState → AreaInstance | 0x218 |
| InGameState → WorldData | 0x270 |
| InGameState → in-game UI | 0xBE0 |
| AreaInstance → LocalPlayerStruct | 0x978; LocalPlayer +0x08 |
| AreaInstance → awake entities | 0xA58 |
| WorldAreaDetails → WorldArea row pointer | 0xC0 |
| Life component → Life / Mana / ES | 0x178 / 0x1C8 / 0x210 |
| ServerData → player inventories | 0xCE70 (`FieldLayout 0x2170 + SKIP 0xAD00`) |
| Inventory → equipped item vector | 0x170 |
| Buffs → effects vector | 0x160 |
| Actor → animation ID | 0x248, Run = 4 |
| ActiveSkillDetails → GrantedEffectsPerLevelDatRow | 0x58 |
| In-game UI → chat parent / chat background color | 0x568 / 0x2B4 |

**Provenance:** The originally supplied GameHelper source snapshot had no verified original repository commit, upstream URL, or license. To avoid redistributing unverified third-party source in the compiled application branch, `develop` **does not include** the C# reference tree; it is archived separately on `reference/gamehelper-poe1`. This project does not compile or link GameHelper. Relevant observations are independently implemented in Rust and documented here. Verify provenance and any attribution/licensing obligations before redistributing that original C# source.

## Final regression and performance review — architecture milestone 6

The final review leaves the shared lifecycle, game-specific flask policies, and
input dispatcher unchanged. The tests added here are regression-only:

- **Shared engine:** startup remains DISARMED; an F11 press while unfocused
  cannot be replayed when focus returns; a held key never toggles repeatedly;
  only a new foreground rising edge changes the armed state.
- **Shared action dispatcher:** existing tests cover focus immediately before
  and after policy validation, denial/error handling, malformed slot rejection
  and successful-only input bookkeeping.
- **PoE1:** existing tests cover offset/vital validation, cache reset, recovery
  and duplicate rotation, effect/charge safety, Low Life rules, optional
  movement/proximity gates and fail-closed census results.
- **PoE2:** the new regression asserts that successful flask input starts the
  existing post-send guard and clears the one-shot charge warning. Existing
  tests check death/CI-like resource handling. The 1000ms guard, 2-slot
  rules and three-bad-sample tolerance have not been changed.
- **Linux:** launcher tests cover no-flag automatic selection, `--debug`,
  backwards-compatible `--auto`, explicit game overrides, mixed/unknown
  Steam AppIDs and ambiguous simultaneous-game discovery.
- **Build:** the feature branch's CI checks Rust formatting, Clippy and unit
  tests for both the Windows helper and Linux launcher, Rust 1.85 MSRV,
  release binaries and AppImage packaging.

**Performance/code-path review:** the common engine uses one 100ms scheduling
loop, one-second process-exit/config checks and two-second binding checks. It
does not parse game memory, scan for entities or allocate a game-wide state
on every tick. The shared `GameSession` caches validated vitals per decision
tick. PoE1 retries offset recovery only at attach/session refresh; its
bounded proximity scanner runs only for explicitly configured eligible
utility rules or the `--debug --nearby` diagnostic and is throttled to
one scan per 750ms. PoE2 continues to use its established lock/charge probes
and guard, avoiding a second behavioral migration during this refactor.

**Remaining quantitative validation:** GitHub-hosted CI cannot measure the
helper's actual Wine/Proton CPU utilization, resident memory, game-process
latency or key-send timing on the player's PC. These must be observed live
in each game at comparable scene density, with normal automation armed and
proximity rules off for the baseline. A lightweight host-side sample is:

```bash
pgrep -af 'poe2-auto-flask.exe'
# Choose the Wine helper's host PID from the list above:
top -b -d 5 -n 4 -p PID
```

Compare the helper's sampled `%CPU` and `RES` while disarmed, armed and
actively using flasks. Recheck PoE1 with an opt-in nearby rule separately if
that feature is used. The PoE1 `--debug` CPU measure is useful diagnostics
but is **not** a direct substitute for measuring the default auto loop.
The PoE1 helper was subsequently sampled during approximately two minutes
of active mapping with Life, Quicksilver and Granite proximity automation:
about 0.44% of one logical core on average (1.5% maximum 2-second sample),
with roughly 20.5 MiB stable resident memory. These are helper-process-only
measurements, not the combined Proton/Wine footprint or a PoE2 benchmark.
CI cannot validate future game offsets.

## Automatic game selection — architecture milestone 5

- **Linux only:** The native launcher inspects candidate running Path of Exile
  processes in `/proc` once at startup, using their Steam AppID
  (`STEAM_COMPAT_APP_ID` and/or `SteamAppId`). Both IDs must agree when
  present. An unknown or conflicting ID is rejected, rather than guessing
  from executable names shared by PoE1 and PoE2.
- Candidates must also supply the actual Steam client path, a valid existing
  Proton `pfx` directory, and an executable Proton tool. The launcher uses
  that candidate's prefix rather than constructing a new one. This adds no
  polling or memory reads to the game automation loop.
- If **exactly one** recognized environment is running, omit `--game` to
  select it. If **both** games are running, implicit selection fails with an
  instruction to pass `--game poe1` or `--game poe2`. If neither qualifies,
  the historical PoE2 fallback remains the selected default but the launcher
  cannot start without a real PoE2 Proton environment.
- An explicit `--game` always selects that game and requires its matching
  Proton environment; it never silently falls back to the other. Without
  mode flags, **both games default to normal automation**, and the shared
  engine still starts **DISARMED** until F11 is pressed while the selected
  game is focused. `--debug` selects read-only diagnostics; `--auto` stays
  an optional backwards-compatible alias. `--nearby` remains PoE1 debug-only.
- The launcher forwards the resolved game explicitly as `--game` to the Wine
  helper. On **Windows**, the helper retains explicit selection and its
  historical PoE2 default; no fragile cross-platform heuristic is added.
- Covered by launcher unit tests for each recognized AppID, conflicting or
  unknown values, no-game fallback, both-game ambiguity, and explicit
  overrides. Cross-game memory reads and flask-policy changes are outside
  this milestone.

## Shared action interface and executor — architecture milestone 4

- Both game-specific policies now submit `ActionRequest::flask(slot, binding)`
  through the single, allocation-free dispatcher in `src/action.rs`. The
  request uses an `ActionKind` enum so future skill and buff actions can
  integrate without embedding their rules in the automation scheduler.
- The dispatcher validates the requested flask-slot range, verifies
  selected-game foreground focus, runs the **game policy's own final
  validation callback**, checks focus again, and sends through the existing
  `SendInput` binding. It does not interpret offsets, read game memory,
  inspect buffs, or schedule additional polling.
- Only a successful SendInput produces `Dispatch::Sent` with the validated
  state and timestamp. A denied/stale/unfocused action is skipped; failures
  distinguish policy validation errors from input errors. Per-game cooldown
  and rotation state is updated **only on successful dispatch**.
- PoE1 keeps the existing immediate pre-send player/chat check, current
  flask-effect and threshold decision, optional Quicksilver motion check,
  state-aware settling and rotation. Invalid game-state validation requests
  reconnection, while input errors remain fatal as before.
- PoE2 keeps the two-slot Life/Mana threshold, charges, active locks and
  1000ms per-flask guard. Its pre-validated decisions also pass through the
  common dispatcher, which now prevents a send if focus is lost during a
  decision. Both Life and Mana can still activate in one tick when eligible.
- No new config values, per-tick game reads, action queue, or autonomous
  background tasks are introduced. The game adapters remain independent.

## Shared automation lifecycle — architecture milestone 3

The auto-mode scheduler is now **one implementation** in `src/engine.rs`,
used by both PoE1 and PoE2. It owns the fixed 100ms polling rhythm, F11
foreground-only edge detection, disarm/focus pauses, configuration reload
(1s), binding reload scheduling (2s), process-exit checking, and failed
session-refresh retries (250ms). It does not interpret game offsets, read
flask memory, or decide which flask/slot to use.

The `GameAutomationPolicy` interface owns the game-specific choices:
- `src/poe1/automation.rs`: five-slot flask inventory, activation settling,
  duplicate rotation, Low Life emergency instant recovery, conditional
  utility proximity and movement, pre-send chat/identity/threshold validation.
- `src/poe2/automation.rs`: existing PoE2 two-slot Life/Mana decisions,
  charge and buff locks, unchanged 1000ms post-send guard and three-sample
  transient read tolerance, plus PoE2 binding-mode handling.

Both policies request validated shared vitals from `GameSession`. PoE1's
specialized state is also retrieved via its session adapter; PoE2's existing
specialized charge/lock probes are retained for now to avoid a separate
behavioral migration. The common scheduler only invokes the policy after
focus, bindings and session refresh gates have succeeded. Each game keeps
its own fail-closed policy and final input validations. No new per-tick
process-memory scans or config fields are introduced. This milestone originally preceded the shared dispatcher and automatic game
selection; both are implemented in the current codebase.

## PoE1 bounded offset recovery — architecture milestone 2

- The PoE1 adapter first validates **previously resolved** offsets (on refresh) or
  the known 2026-09 layout (initial attach). This unchanged path does no
  heuristic scanning and keeps the normal 100 ms vitals read compact.
- If those fail *after* the GameStates signature and active InGameState have
  been validated, it searches only small, aligned windows: InGameState →
  AreaInstance **±0x20 (8-byte steps)**; InGameState → WorldData **±0x20
  (8-byte steps)**; AreaInstance → PlayerInfo **±0x20 (8-byte steps)**; and
  the common offset of the three adjacent Life/Mana/ES pools **±0x20
  (4-byte steps)**. It does **not** guess offsets throughout arbitrary
  process memory or scan unknown entity/component types.
- A candidate must contain a plausible ServerData/local-player pair,
  actual `Metadata/Characters/` metadata, a valid player entity ID,
  a resolved Life component with valid health/mana/ES pools and **positive
  current Life** during heuristic discovery, and an independently decoded
  WorldArea.dat row (name, ID, flags and act). Known-offset reads still
  support a dead character without requiring positive current Life.
  The full player and area chains are checked again before acceptance.
  **Ambiguous or absent matches fail closed**, without authorizing input.
- The accepted offsets stay in the PoE1 session probe and are used for
  normal polling, Flask PlayerInfo verification and WorldData checks.
  Cache/offset discovery occurs only on attach or session refresh. A
  changed validated layout prints one `PoE1 validated recovered offsets`
  terminal line with the chosen offsets, to aid troubleshooting.
- **Limits:** this does not automatically repair the GameStates byte
  signature, arbitrary Life pool stride changes, player/buff/flask/chat/
  inventory/entity structure offsets, or a substantial executable update.
  These still require a manually verified offset update. Any failed
  mandatory reader blocks automation, even if partial state looks plausible.

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
- Duplicate flasks **automatically rotate by least-recently-used eligible slot** within the same base type. The first selection favors most complete remaining uses (charges / effective cost), then most charges, then the lowest slot. Every successful activation advances a tiny fixed five-slot history. Two or more Quicksilvers or Life flasks alternate when eligible and charged. An active effect ordinarily blocks a duplicate of the same type; the only exception is an eligible fully instant **Life** flask below the emergency threshold while a real Life recovery effect is already active.
- **Instant Life/Mana recovery protection:** There is **no fixed 900 ms cooldown**. After each successful Life or Mana key, a **one-tick (minimum 100 ms) same-resource settling guard** briefly blocks all copies of that type. On subsequent ticks the guard releases as soon as the sampled flask charges decrease, Life/Mana increases, or there is fresh loss of that resource since the last observation. The usual threshold, charge and *real* active-effect checks then decide whether another activation is needed. When charges and the resource are unchanged, the guard continues to block unproductive repeat keys, even after 100 ms. This uses only existing vitals/flask snapshots, without extra process-memory reads. The original **350 ms per-slot** guard remains for utilities only; it does not hold back Life/Mana. Life, Mana, and utilities are independent. This is input safety, **not** an in-game flask cooldown or healing-amount optimizer.
- A flask's raw `Charges.PerUseCharge` may differ from actual charge cost. The reader uses the player's `flask_charges_used_positive_percentage` stat (ID `0x2E5`) and applicable explicit flask charge-use modifiers to calculate an effective, truncated integer cost. Unknown cost data blocks the affected flask rather than guessing.
- Flask recovery modifiers are classified once per equipped item, in the **same explicit Mods.dat pass already used for charge-cost modifiers**. The cached values are Gradual/unknown, fully Instant, Instant-on-Low-Life, or PartialInstant. Only positively recognized fully instant recovery qualifies for the bypass; partial/unknown cannot. PoE1 **Low Life is current Life <= 50% of actual maximum Life**, rather than 50% of the unreserved resource pool. At a 60% normal threshold a conditional-instant Life flask is held until actual Low Life, while ordinary Life flasks remain eligible at 60%. This rule doesn't raise the configured normal threshold if set below 50%. The debug reader reports each flask's inferred recovery kind; verify it for actual equipped flask prefixes before treating it as fully reliable.
- **Life-only emergency exception:** while a genuine, active Life recovery buff exists, an eligible fully instant Life flask can bypass same-type active-effect blocking if Life is at or below **min(25%, half the configured Life threshold)**. Example: with normal Life 50%, ordinary gradual recovery begins at <=50%, and a conditional-instant copy can fire during that recovery only at <=25% Life. A pending/uncorroborated Life input still blocks emergency bypass. After the emergency, normal least-recently-used selection resumes. **Mana is unchanged**: any active Mana flask effect still blocks another Mana flask regardless of its modifier. No new TOML flags, no new per-tick mod reads.
- Partial instant recovery remains separately identified but cannot bypass an active gradual effect yet. Unique/enchantment granted recovery conditions and healing-amount comparisons remain unverified and are not used to justify emergency input.

**Emergency-flask live test:** Test one ordinary Life flask and a known instant/instant-on-Low-Life Life flask with the normal threshold at 50%. Confirm they still rotate when no recovery is active, that an existing gradual effect blocks the second flask around 30-40%, and that the instant copy can trigger at <=25%. At a 60% normal threshold verify the conditional-instant copy does not fire at 55%, but does at 50%. The PoE1 debug output should identify its recovery type. If it says gradual/unknown, the modifier ID needs live verification before relying on that exception. Only test safely with manual recovery available.

**Multiple Flask live test:** Equip two Quicksilvers (or two Life flasks). Start `--game poe1 --auto`, arm with F11, and monitor successive output slots: after the first activation, an eligible different duplicate should be selected when the first effect expires. Use up one copy's charges to verify fallback. With two instant Life (or Mana) flasks, verify unchanged low health with *no observed charge consumption or recovery* does not repeat the same input, but confirmed charge consumption or a second large hit/drain allows a prompt response on the next eligible 100 ms loop. Ordinary active recovery effects still suppress same-type duplicates. Changes to flask inventory or area use the session's existing cache refresh; rotation resets on rearm and refresh. Both buffs and resource reads remain the source of truth.

## Optional Quicksilver movement gate

GameHelper's default Quicksilver rule used `PlayerAnimation.Equals(Animation.Run)` with a `Wait(1)` condition. This implementation samples `Actor.AnimationId` at `Actor+0x248` only if the option is on. Run = 4. It requires uninterrupted observed Run animation for the configured duration. Idle/cast animations, unrecognized animation, stale sample gaps, loss of focus, zone changes, disarming and invalid state reset the timer; the animation is checked again immediately before an input.

The gate only affects Quicksilver; it never prevents Life, Mana or another utility flask from being considered. Continuous movement is more reliable than assuming held left-click means actual character motion. Stutter-step casting builds may need a lower duration (e.g. 250 ms) or may leave the gate disabled.

## Safety, validation, and limitations

- Starts **DISARMED**; F11 only arms/disarms while the selected game is focused.
- Game state, focus, town/hideout permission, chat/UI visibility, death status, grace period, charges, active effects, modifier validity, a **350 ms utility settling guard**, and a **state-aware one-tick Life/Mana settling guard** gate input. This is not a flask cooldown. A Quicksilver use never delays a Life/Mana response. Invalid observations fail closed. The launcher does not invent a missing Proton prefix.
- `--game poe1 --debug` reads state only. It reports Life/Mana/ES, area, chat open/closed, base flask type, charges, raw/effective charge cost, active effects and Actor movement.
- **Live-tested successfully on a user's Steam Proton setup:** Life/Mana/ES and unreserved totals, area changes, five flask slots, matching player buffs, active Utility Flask effects, 26%-reduced Life flask cost (raw 15 → effective 11), chat detection, automatic PoE1 keybindings, Quicksilver and Life automation, F11 arming, focus and town/hideout protection, config reload, and optional ~750 ms Run gate.
- **Still needing broader live testing:** all 18 utility types, duplicate Life/Mana/utility selection, unusual/unique flask modifier behavior, gamepad input, other movement skills, and PoE2 end-to-end regression. Automated CI tests cannot prove future game versions preserve memory offsets.
- The reference C# snapshot is deliberately excluded from the Rust source and distributions. Any future inclusion requires licensing/provenance verification.

## Experimental read-only nearby-monster census

GameHelper's reference `NearbyMonsterInfo` uses the already-maintained awake entity collection to count monsters by Normal/Magic/Rare/Unique rarity in two distance circles. This app does **not** maintain a game-wide entity collection, so a full entity tracker would be an unnecessary baseline cost. Its experimental census runs on explicit request: as read-only diagnostics
or for an enabled PoE1 utility proximity rule. The diagnostic command is:

```fish
./poe2-auto-flask-x86_64.AppImage --game poe1 --debug --nearby
```

The `--nearby` command-line flag is **diagnostic-only** and requires PoE1 `--debug`; it does not activate flasks. Separately, an explicitly configured `[poe1.nearby.<type>]` condition may request the same bounded census for that utility flask during normal PoE1 automation. With no such configured condition, normal PoE1 and PoE2 automation do not scan monsters.

When enabled, the reader traverses only the PoE1 **awake** entity map (not sleeping or world decorations), with a hard limit of **1,500 map nodes** and **512 nearby monsters**. It caches static entity type/component addresses for the current area, limits remote rescans to once per **750 ms**, and groups the surviving monsters by rarity and proximity. It excludes dead/friendly/untargetable entities and known noncombat categories, including Harbingers and Tormented Spirits. It shows counts in **60** and **100 grid units** (same coordinate convention as GameHelper's circle distances). It fails closed on malformed/changing maps or invalid state; no partial result authorizes automation.

**Live validation (2026-11-11):** Normal, Magic, and Rare proximity counts were consistent with gameplay and friendly entities were excluded. During Kitava's Act 10 boss encounter, one Unique was counted before becoming vulnerable and the heart then appeared as a second Unique while still immune to damage. This is consistent with PoE's distinct attackable phases (Kitava and exposed Heart) and demonstrates that the current life/targetable flags are **not sufficient to prove an enemy can currently be damaged**. This is not fixed by pretending Unique always means combat: the scanner is experimental and optional proximity-gated utility use may fire during an invulnerable boss phase. Before enabling proximity-gated flasks, treat Unique-only rules conservatively and consider specific noncombat/invulnerable state checks (especially phased bosses). <https://www.poewiki.net/wiki/Kitava,_the_Insatiable>

**Performance instrumentation:** Normal PoE1 `--debug` and `--debug --nearby` now both report `Helper CPU: X% of one core (Y ms CPU / 5.0s wall)` at roughly five-second intervals, measured with Windows `GetProcessTimes` inside the helper under Proton. `--nearby` additionally prints `Nearby last full scan: Z ms wall` beside the rarity counts. This wall time includes cross-process read latency and **is not the scanner's CPU time**. The CPU percentage covers the helper's entire process (including logs and all readers), not PoE or the Wine server process. It is relative to one logical core, not system-wide Task Manager percentage; compare the two modes at similar monster density and area. Metrics are diagnostic-only and cannot trigger a flask. Under unsupported Wine configurations the CPU measure explicitly prints unavailable.

**Important:** This remains an experimental candidate scanner. Boss-specific targetability and dense-map traversal need further validation. It runs only with `--debug --nearby` or when an explicitly enabled PoE1 utility proximity rule needs it; default automation without proximity rules performs no monster scan. Enabled utility rules reuse one shared cached census.

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

## Current development workflow

The PoE1 feature branch was squash-merged into `develop` and subsequently released
on `main` as v0.8.0. After that release, cross-game naming/layout cleanup
takes place on **`develop` only**. Keep the public `main` branch and its
release tags untouched until the next release is explicitly approved.
See [Source layout and runtime audit](SOURCE_LAYOUT.md) for current module ownership.

## Writhing Jar unique flask (develop)

PoE1-only, **disabled by default**. The Writhing Jar is a unique Hallowed Hybrid Flask.
The optional policy identifies its worm-spawning modifier family `SummonsWormsOnUse`
from the cached flask Mods.dat walk, instead of identifying every Hallowed Hybrid Flask
as a Jar. The exact live Mods.dat identifier should be checked in game with `--debug`
before relying on automatic activation. Missing or unrecognized mods fail closed.

```toml
[poe1.unique]
writhing_jar = true
```

When enabled and positively identified, The Writhing Jar activates when it has at least
one **verified effective charge cost** available, even at full Life and Mana, subject
to the normal town/hideout, alive, focus, chat, grace-period, disarmed and preflight
checks. Life/Mana recovery has higher priority. Other hybrid flasks remain manual-only.

Optional proximity requirements share the same throttled (750 ms) monster census
as utility flasks:

```toml
[poe1.unique_conditions.writhing_jar.nearby]
enabled = true
radius = 60
minimum_count = 1
minimum_rarity = "normal"
```

The nearby table is separate from the Boolean toggle because TOML cannot have
both `writhing_jar = true` and `[poe1.unique.writhing_jar.nearby]` at once.
Absent or `enabled = false` means **charges-only** activation. An unavailable
census fails closed only when proximity is enabled.

After a successfully sent Jar key, the helper waits to observe reduced charges
before allowing another Jar activation; a lost input cannot repeat indefinitely
on the normal utility settling timer. The current 350 ms per-slot utility
settling remains active. No extra per-tick modifier walk was added.
