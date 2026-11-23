# PoE1 support

PoE1 uses an independent validated reader and automation policy within the
shared Rust backend. It supports five flask slots and opt-in skill presets.
See [Source layout](SOURCE_LAYOUT.md) for module ownership,
[Profiles](PROFILES.md) for storage and [UI architecture](../app-ui/README.md)
for presentation.

## Connection and memory safety

The Steam AppIDs are **238960** (PoE1) and **2694490** (PoE2). The Linux
launcher discovers the running game's Steam root, prefix and Proton and uses
`proton runinprefix`. With both games running, choose explicitly with
`--game poe1` or `--game poe2`. Native Windows uses a unique client's install
directory; an unfamiliar directory prompts for selection in the UI. Debug
mode retains the console prompt. No dual-parser memory probing is used.

`GameSession` dispatches to separate game adapters. Common vitals use
`VitalPool`, `LiveVitals` and `GameSnapshot`; each game validates its own data.
The PoE1 adapter owns validated player identity and demand-driven area,
flask, buff, movement, skill and proximity caches. Required reads validate the
in-game state, local-player identity and metadata.
Changed identities or memory I/O failures refresh the session. Unknown or
stale observations cannot authorize input.

The original C# reference tree is not bundled; see
[acknowledgements](../ACKNOWLEDGEMENTS.md) for research provenance.

## Configuration and supported flasks

Edit the active profile in the UI or `profiles/*.toml`. Life/Mana use the
shared `[health]` and `[mana]` thresholds against the unreserved pool.
Their defaults are 60% Life and 30% Mana, enabled as rules; overall automation
still starts disabled. All utility/unique/skill rules default off.

```toml
[poe1]
quicksilver_min_run_ms = 750

[poe1.utility]
quicksilver = true
granite = true
jade = false

[poe1.nearby.granite]
enabled = true
radius = 60
minimum_count = 1
minimum_rarity = "normal"
```

The 18 utility bases are Diamond, Ruby, Sapphire, Topaz, Granite, Quicksilver,
Amethyst, Quartz, Jade, Basalt, Aquamarine, Stibnite, Sulphur, Silver, Bismuth,
Gold, Corundum and Iron. Unknown bases and ordinary Hybrid flasks remain
manual-only. The UI lists all supported types; enable only the ones needed.

Tinctures can share the flask bar with supported flasks. They remain manual
and do not prevent automation from using the other equipped flasks.

Keys come from `Documents/My Games/Path of Exile/production_Config.ini`,
using all five `use_flask_in_slotN` entries in the selected input section.
Invalid, ambiguous, missing, controller, mouse-button or duplicate flask
bindings prevent input. Moving a supported flask between slots needs no
helper configuration change.

## Selection, recovery and charge costs

- Life/Mana take priority over utility flasks. A resource action must meet its
  configured threshold; a CI-like Life pool prevents automatic Life flasks.
- Eligible duplicates rotate by least recently used slot. Initial preference
  is most complete remaining uses, then charges, then lowest slot. Histories
  advance only after successful input and reset on enable/session refresh.
- Life/Mana have a one-tick (at least 100 ms), state-aware settling guard.
  Observed charge consumption, recovery or fresh resource loss can release
  the guard; unchanged charges/resources block unproductive repeat input.
- Utilities retain a 350 ms per-slot settling guard. Quicksilver cannot delay
  an emergency Life/Mana response. These are input guards, not game cooldowns.
- Effective charge costs use validated player/flask charge-use modifiers,
  truncated to an integer. Unknown cost data blocks the affected flask.
- Equipped-item modifier classification is cached. Fully instant,
  instant-on-Low-Life and partial/unknown recovery remain distinct.
  Conditional instant waits for actual Low Life (at most 50% of total Life).
- An active same-resource recovery effect normally blocks another flask.
  Only positively identified fully instant Life can bypass an existing
  gradual Life recovery at **min(25%, half the configured threshold)**.
  Pending uncorroborated input still blocks that bypass. Mana has no such
  exception; partial/unknown recovery cannot justify it.

The optional Quicksilver movement requirement accepts **0** (off) or
**100–5000 ms**. It observes continuous Run animation and resets on idle,
casting, stale samples, focus/area changes or disabled automation. It checks
movement again before input. Other flask types are unaffected.

## Nearby conditions

Utility/unique proximity and enabled skill conditions share one on-demand
awake-entity census, throttled to **750 ms**. It is bounded to **1,500 map
nodes** and **512 nearby monsters**, with traversal/time limits and cached
static component information. A malformed, changing or oversized scan fails
closed; partial counts cannot authorize an action. No enabled consumer means
no scan. Life/Mana decisions do not wait for a monster census.

Nearby radius accepts **1–150 grid units**, default **60**. Count accepts
**1–512**. Minimum rarity is inclusive: Normal includes all rarities, Magic
includes Magic/Rare/Unique, Rare includes Rare/Unique, and Unique includes
only Unique. A single Unique does not override a count greater than one.

A table gates only its matching flask type. `enabled = false` retains the
condition values without gating; omitting it restores ordinary effect
maintenance. Unknown types and invalid ranges are rejected.

The scanner excludes dead/friendly/untargetable living-monster candidates and
known noncombat categories such as Harbingers and Tormented Spirits.
**Targetable does not prove damageable**: live testing counted Kitava and
its Heart before vulnerable phases. Unique-only rules can therefore trigger
early. No damageability/aggro guarantee is made.

## Writhing Jar

The opt-in Writhing Jar rule identifies `SummonsWormsOnUse` in equipped mods,
not every Hallowed Hybrid base. Unrecognized mods fail closed. When enabled,
it can activate at full Life/Mana if a verified charge cost is available and
normal safety gates pass. Life/Mana recovery has priority.

```toml
[poe1.unique]
writhing_jar = true

[poe1.unique_conditions.writhing_jar.nearby]
enabled = true
radius = 60
minimum_count = 1
minimum_rarity = "normal"
```

The condition table stays separate from the Boolean toggle. Without an
enabled condition, eligibility is charges-only. After successful input the
rule waits for observed charge consumption before allowing another Jar use;
the utility settling guard remains active.

## Skill presets

Skill presets use declarative rules on the same 100 ms scheduler, with
attempt guards and shared preflight. The UI lists skills alphabetically.
Keys must match in-game bindings and can include Ctrl, Shift or Alt. Enabled
skills must use distinct bindings and cannot use the automation toggle key.
Flask-key conflicts are warned on configuration/binding changes.

- **Bladefall of Trarthus:** starts the sustained skill when its effect is
  absent and the game reports it usable. Its only settings are Enable and
  Keybind; the game handles resource costs, including build-specific changes.
- **Blood Rage:** minimum-Life threshold, absent-buff check and optional nearby
  enemies; nearby gating defaults on when the preset is configured.
- **Convocation:** uses the configured key whenever the game reports the skill
  usable and off cooldown. Only Enable and Keybind are configurable; the usual
  automation safety checks and skill attempt guard still apply.
- **Immortal Call:** maximum-Life or optional maximum-Shield threshold and
  validated skill readiness. The game controls Endurance Charge consumption;
  no minimum charge count is required by the preset.
- **Molten Shell:** maximum-Life or optional maximum-Shield threshold and
  validated skill readiness.
- **Spirit Offering:** usable Actor skill, absent `offering_spirit` player buff
  and enough nearby corpse candidates, with a one-second attempt interval.
- **Steelskin:** maximum-Life or optional maximum-Shield threshold, like
  Molten Shell. All three Guard presets share an attempt guard and avoid
  casting over any of their active Guard effects. Their shared cooldown remains
  game-controlled.

All three Guard presets trigger when either percentage is at or below its limit.
`threshold_percent` is Life (1–99); `shield_threshold_percent` is Shield
(0–99, default 0). **0 disables the Shield condition.** An absent or zero-maximum
Shield pool is ignored. A depleted pool with a positive maximum still qualifies
when its Shield condition is enabled. Life takes priority when both match,
including when choosing between Guard presets. A shared attempt guard
permits only one Guard key press; active effects and game readiness still prevent
recasting when the other resource later falls below its threshold.

All presets default off. Bladefall does not receive repeated key presses while
its sustained effect is present, including effects without a countdown. If it
ends, the preset can reactivate it after the ordinary checks and attempt guard.

```toml
[poe1.skills.spirit_offering]
enabled = true
key = "E"
nearby_radius = 60
nearby_minimum_count = 1
```

Corpse candidates are zero-Life, nonfriendly, targetable awake monster entities
within the configured radius. They are counted separately from living
monsters, using the same census. This is not proof that the game accepts a
candidate as consumable (for example, shattered or consumed corpses).
The helper sends the configured key; it does not aim, move the cursor or
account for cast time. Proximity does not guarantee cursor targeting.
Interrupted casts can retry after the attempt interval while the buff remains
absent; a confirmed active offering buff blocks another attempt.

## Diagnostics, validation and limitations

```sh
./poe2-auto-flask-x86_64.AppImage --game poe1 --debug
./poe2-auto-flask-x86_64.AppImage --game poe1 --debug --nearby
```

Both modes are read-only and send no automation input. The second reports
monster rarity counts and corpse candidates at 60/100 units, plus optional
skill readiness. Normal diagnostics include vitals, area, chat, flasks,
charges/costs, effects and movement. `Helper CPU` reports process CPU relative
to one logical core over roughly five seconds; census wall time is remote
scan duration, not CPU time. These diagnostics remain supported tooling.

Input remains gated by foreground focus, allowed area, living player,
chat/UI state, grace period, valid bindings, charges, effects and fresh
preflight. An isolated malformed inventory can suppress flasks while other
independently validated actions remain available. Missing nearby state denies
only actions that require it. Controller/gamepad automation and mouse-button
flask bindings are unsupported.

Automated tests cover pure decisions, guards, readers' structural validation,
config/profile behavior and dispatch failure paths. User live PoE1/Proton
testing covers connection, vitals/name/area, binding discovery, common flask
and nearby rules, configuration changes, focus/area safety and UI/tray
integration. This does not certify every utility/modifier/build or future
game updates. Broader platform checks are deferred; the final external
performance review follows testing the `develop` CI artifacts.
