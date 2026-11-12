# Source layout, conventions and runtime audit

This is the post-v0.8.0 structural cleanup baseline, developed on `develop`.
It is not a new flask algorithm or a release. All existing game decisions,
offset values, memory reads, input dispatch and timing rules remain authoritative.

## Ownership rule

A module belongs inside `poe1/` or `poe2/` if it interprets that game's
memory layout, executable details, bindings, effects, flasks or game-specific
decision rules. The `src/` root owns only cross-game infrastructure and
orchestration. Similar concepts use the same file names when practical; different
game mechanics do **not** imply the same implementation.

| Shared `src/` | Responsibility |
| --- | --- |
| `main.rs`, `game.rs`, `session.rs` | CLI, game selection, adapter dispatch and shared per-tick state |
| `engine.rs`, `action.rs` | The single 100ms lifecycle/arming loop and validated input dispatcher |
| `game_state.rs`, `area.rs` | Common vitals/snapshot contract and area data type |
| `config.rs`, `bindings.rs`, `file_watch.rs` | Shared settings manager, ini binding parser, Windows Documents lookup and change detection |
| `process.rs`, `entity.rs`, `input.rs`, `platform.rs` | Windows process access, named-component resolution, input primitives, platform checks |

| Matching game files | Responsibility |
| --- | --- |
| `mod.rs` | Small module index and game executable-name list |
| `reader.rs` | Game-specific validated memory probe, per-game vital decoding and reconnect |
| `offsets.rs` | Game-specific fixed memory addresses and signature definitions |
| `layout.rs` | Bounded layout-offset candidate search at attach/refresh |
| `adapter.rs` | Implements common `GameStateReader` interface, owns game-specific caches |
| `area.rs`, `flasks.rs`, `buffs.rs` | Game-specific area, flask and status-effect readers |
| `bindings.rs` | Game-specific config discovery and bindings lifecycle |
| `automation.rs`, `debug.rs` | Game-specific auto policy and read-only diagnostics; each exposes `run` |

PoE1 additionally owns `config.rs` (utility, movement and proximity options),
`activation.rs`, `charge_cost.rs`, `chat.rs`, `decision.rs`,
`movement.rs`, `nearby.rs`, `recovery.rs`, `rotation.rs` and `types.rs`
because those mechanics do not currently have identical consumers in PoE2.
Its long decision and activation test suites are separated into
`decision_tests.rs` and `activation_tests.rs`.
PoE1 also owns `state.rs` (one demand-driven tick snapshot and shared
pre-dispatch verification), `skills.rs` (validated Actor skill readiness)
and `skill_rules.rs` (skill IDs, declarative conditions, preset registry
and per-skill attempt guards). Molten Shell is the first registered preset,
not a special scheduler branch.

## Terminology

- `Probe` describes validated game-specific pointer addresses and layout;
  both games expose `probe(process)`, `refresh(process, previous)`, and
  `sample_vitals(process, probe)` from their private readers.
- `Adapter` provides game state via `GameSession`; the common state uses
  `LiveVitals`, `VitalPool`, `GameSnapshot` and `AreaInfo`.
- `Policy` implements the shared `GameAutomationPolicy` contract; only
  policies select actions. Input itself goes through `action::dispatch`.
- `Snapshot` is sampled observation, `Cache` is durable reusable resolution,
  `Guard` prevents premature repeats, `State` holds policy lifecycle.
- Two-slot PoE2 and five-slot PoE1 logic remain distinct. Do not make a
  common `FlaskSlot` merely because both have charges.

## Duplicate-work and hot-path review

| Path | Current work | Review result |
| --- | --- | --- |
| Shared engine | One 100ms tick, process/config check every 1s, bindings every 2s, retry on stale pointers | No duplicate automation loop or game-memory parser |
| Shared session | Cache common vitals once per decision tick; PoE1 adapter caches area/flask/effect/animation snapshots per tick | Avoids multiple full reads of the same cached PoE1 snapshot |
| PoE1 decision / pre-send | One demand-driven `TickState` caches vitals/area/chat/effects and selected game observations. A single fresh `Preflight` verifies vitals/chat/effects once for all candidates; dispatch checks focus for every key | Avoids repeating full memory reads/validation independently for each action while retaining a safety boundary |
| PoE1 flask reader | Checks identity/inventory shape and live charge state; caches item identities and component/modifier classification | Identity/charge reads are required to fail closed on equipment or area changes |
| PoE1 optional skill rules | `configured_rules` registers enabled presets; pure conditions select eligible skills from `TickState`. Skill-readiness is requested only after cheaper conditions pass | No skill Actor scan when all skill presets are disabled; flask and skill candidates can both dispatch in one 100ms tick |
| PoE1 proximity | On-demand bounded awake-monster walk; caches static component addresses and throttles scans to once per 750ms | No global entity tracker; off when there is no eligible configured rule |
| PoE2 auto | Samples existing two-slot status locks and charges, caches charge-component locations, enforces a 1000ms guard | Existing fast reader preserved without new per-tick overhead |
| PoE2 debug | Uses a deliberately fuller reader for detailed effects/inventory diagnostics | Runs in read-only debug mode, **not** in addition to fast auto sampling |
| PoE2 layout | Shared one identical known-offset-first helper previously repeated in two modules | Actual source duplication removed without changing search order |
| Linux launcher | Identifies game and active Proton environment at startup | No per-tick Steam/process discovery overhead |

The application deliberately samples some values twice only when an action is
actually selected: once for the tick decision, then once for the shared
pre-dispatch safety boundary. Flask and skill actions reuse the same preflight
values, rather than re-reading vitals/chat/effects for each action separately.
Foreground focus is still checked around each key, including the second action
in a tick. PoE1's first registered skill is Molten Shell; each future preset
adds its own conditions without another automation loop. The skill reader now batches all requested preset IDs into a single
Actor ActiveSkills traversal and a single cooldown traversal for the decision
snapshot, and (only when selected) a fresh bulk pre-dispatch check. It
inspects the character's current Actor skills, not the game's complete skill
database. Static skill-definition names remain cached; live usability is
read fresh. Rules whose cheap conditions fail trigger no skill scans. The
currently configured preset is Molten Shell; the second skill ID in unit
tests demonstrates independent result classification before additional
presets are enabled. Do not infer hotbar assignment from presence in the
Actor skill list. Rechecking game focus, player identity, charges or area boundaries is a
deliberate fail-closed requirement. PoE2's debug and optimized auto readers are separate
workloads, not duplicate work performed every tick. Some small helper functions
retain per-game validation/error semantics rather than being generalized
prematurely.

The earlier PoE1 live map measurement (Life/Quicksilver/Granite and nearby
scanning enabled) used approximately 0.44% of one logical CPU core on average
and about 20.5 MiB resident memory for the helper alone over two minutes.
That is **pre-cleanup** evidence; it is not a fresh benchmark of this refactor.
PoE2 live validation remains deferred because PoE2 is not installed.

## Regression and promotion boundaries

CI checks formatting, Clippy, Rust 1.85 compatibility, Windows helper unit
tests and a Windows release build; Linux launcher tests/builds are conditional
on changes to the launcher or packaging. These tests cannot certify game
memory offsets after future game updates. Do not modify `main`, publish a
release, change flask thresholds, remove validation checks or introduce
steady-state scans as part of source-layout cleanup. After this large move,
retest PoE1 startup, arming/focus, Life/Mana, utility/proximity, area/chat
pauses and transitions before promoting out of `develop`. Revalidate PoE2
when available.
