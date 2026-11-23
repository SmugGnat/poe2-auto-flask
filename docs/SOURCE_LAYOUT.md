# Source layout

Game-specific memory, bindings and decisions belong in `src/poe1/` or
`src/poe2/`. The root `src/` owns cross-game infrastructure and orchestration.
The UI and launcher consume the existing backend; they do not implement
another automation loop or game-memory parser.

## Crates and entry points

| Location | Responsibility |
| --- | --- |
| Root `Cargo.toml`, `src/lib.rs` | Shared backend/configuration library |
| `src/main.rs` | Development console entry point and read-only diagnostics |
| `app-ui/` | Production Windows GUI executable, including backend thread |
| `linux-launcher/` | Native Linux Steam/Proton discovery and app launch |

All three crates inherit `rust-toolchain.toml` and have committed lockfiles.
The shipped Windows executable is built from `app-ui/`, not the root console
binary. The AppImage bundles that same Windows application.

## Shared Rust modules

| Files | Responsibility |
| --- | --- |
| `backend.rs`, `launch.rs`, `game.rs` | Startup, CLI options, explicit/Steam game identity and attach/reconnect |
| `runtime.rs` | Bounded status/command channels and authoritative connection/automation snapshots |
| `engine.rs`, `action.rs` | Single 100 ms automation lifecycle and validated input dispatch |
| `session.rs`, `game_state.rs`, `area.rs` | Adapter dispatch, per-tick common vitals/snapshots and area contract |
| `config.rs`, `profiles.rs`, `file_watch.rs` | Configuration validation, global/profile persistence and file-change checks |
| `bindings.rs`, `input.rs` | Shared INI/key parsing, Windows Documents lookup and input primitives |
| `process.rs`, `entity.rs`, `platform.rs` | Windows process reads, named components and platform checks |
| `telemetry.rs` | Independent presentation-only read session, outside automation |
| `diagnostics.rs` | Read-only diagnostics and failure/CPU reporting |

Windows-only readers/input are gated by `cfg(windows)`. `poe1/shared.rs`
exposes configuration/types to non-Windows builds without compiling memory
readers. See [UI architecture](../app-ui/README.md) for frontend ownership.

## Game-specific modules

Both games use `mod.rs`, `reader.rs`, `adapter.rs`,
`area.rs`, `flasks.rs`, `buffs.rs`, `bindings.rs`, `automation.rs` and `debug.rs`.
These modules have matching responsibilities and game-specific behavior.
Two-slot PoE2 and five-slot PoE1 behavior stay distinct.

PoE1 additionally owns utility/skill configuration, charge-cost/recovery
classification, duplicate rotation, chat, movement, nearby counts, optional
character-name reads and skill readiness. Its `state.rs` caches demand-driven
tick data and one shared pre-dispatch verification. `skill_rules.rs` holds
preset rules and attempt guards. Longer pure-policy tests live alongside the
implementation in `activation_tests.rs` and `decision_tests.rs`.
`nearby_filters.rs` holds GameHelper encounter exclusions and temporary-state
eligibility rules used by the shared nearby census.
`nearby_debug.rs` adds opt-in Unique-boss observations and change reports to
read-only diagnostics; automation never consumes these observations.

## Runtime boundaries

- The automation engine runs every 100 ms, checks process/configuration once
  per second and bindings every two seconds.
- The common session caches vitals once per decision tick. Game-specific
  adapters own their own validated caches and invalidation.
- Selected actions share one fresh preflight snapshot; focus is still checked
  around each dispatched key. No stale observation authorizes input.
- PoE1 proximity/corpse scans run only on demand, at most once per 750 ms.
  No enabled consumer means no monster scan.
- PoE2 retains its existing optimized two-slot charge/effect reader and guard;
  detailed debug readers are separate read-only workloads.
- Visible UI telemetry uses another read-only session at most twice per second.
  Hidden/minimized UI stops presentation polling. It never holds a lock over
  automation reads or sends input.
- Linux process/Steam discovery occurs at launch, outside all automation ticks.

See [PoE1 support](POE1_SUPPORT.md) for game-specific safety and limitations,
[Profiles](PROFILES.md) for persistence and the [root README](../README.md)
for checks, artifacts and the `develop` → `main` release workflow.
