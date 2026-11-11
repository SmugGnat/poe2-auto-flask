# PoE1 utility-flask coverage and optional movement gating

This change extends the PoE1 experimental auto-flask feature, preserving its proven recovery-flask rules and existing PoE2 behavior.

## All standard utility flasks

All 18 standard numbered PoE1 `FlaskUtilityN` metadata base items are classified by the actual item path (not inventory slot or guessed charge cost).

| N | Name | N | Name |
| --- | --- | --- | --- |
| 1 | Diamond | 10 | Basalt |
| 2 | Ruby | 11 | Aquamarine |
| 3 | Sapphire | 12 | Stibnite |
| 4 | Topaz | 13 | Sulphur |
| 5 | Granite | 14 | Silver |
| 6 | Quicksilver | 15 | Bismuth |
| 7 | Amethyst | 16 | Gold |
| 8 | Quartz | 17 | Corundum |
| 9 | Jade | 18 | Iron |

Base-item IDs are documented by PoEDB: https://poedb.tw/us/Utility_Flasks and individual base pages (e.g. https://poedb.tw/us/Jade_Flask and https://poedb.tw/us/Silver_Flask). The four previously verified types retain their existing numeric mapping.

Existing config files continue to parse. Each utility is independent, opt-in and off by default. To enable Jade and Silver, for example:

```toml
[poe1.utility]
quicksilver = true
jade = true
silver = true
```

The future UI need not show 18 permanent toggles: it can inspect `GameSession` and show only currently equipped utility types, with a way to expand all types or view advanced rules. Current TOML remains explicit and backwards compatible. Unknown utility base IDs and Hybrid recovery flasks remain manual-only.

## Movement-gated Quicksilver

GameHelper's `GameOffsets/Objects/Components/Actor.cs` documents `Actor.AnimationId` at `Actor+0x248`. Its `RemoteEnums/Animation.cs` identifies Run = 4. Its AutoHotKeyTrigger starter profile uses `PlayerAnimation.Equals(Animation.Run)` with `Wait(1)` for Quicksilver. This implementation reads only that 32-bit animation field; it **does not** read GameHelper's skill/cooldown arrays or other entities.

The optional setting:

```toml
[poe1]
quicksilver_min_run_ms = 750

[poe1.utility]
quicksilver = true
```

`quicksilver_min_run_ms = 0` (default) preserves the existing behavior. A nonzero value from 100 to 5000 milliseconds requires that the character has been observed in the Run animation for that long without interruption. A sample gap over 350 ms, non-Run animation, failed Actor read, loss of game focus, area change, disarm, or invalid game state resets the timer. The animation is checked again immediately before sending the Quicksilver key. This gate affects **only Quicksilver**, not recovery or other utility flask types.

This is deliberately stricter than "left mouse button held": the latter can represent clicks in chat or UI and is not a reliable indication of character movement. Running via movement skills, teleporting, or attack-moving may not hold the Run animation continuously, so the feature is optional until tested in the live PoE1 client.

## First live test

1. Run `--game poe1 --debug`. Observe the new `Movement: Run (Actor animation ID 4)` when continuously walking/running, and `Movement: not running (Actor animation ID ...)` when idle. Check movement skills too; an unavailable status means the optional gate must not activate any Quicksilver flask.
2. Keep `quicksilver_min_run_ms = 0` until the diagnostic consistently matches actual running.
3. Set `quicksilver_min_run_ms = 750` and keep `quicksilver = true`. Start `--game poe1 --auto`, arm with F11, verify no Quicksilver use while stationary, then sustained running triggers one use when the effect isn't active.
4. Try stopping, starting, tabbing away, chatting, changing zones and using a movement skill; the Quicksilver should not fire until a new continuous run interval has been observed.
5. Opt in a Jade or Silver utility (if equipped), verify its type and buff activation and duplicate policy. Other utility mappings are sourced from base-item data but haven't been individually confirmed against a live client.

All utility toggles default off, and the movement gate defaults off; neither change silently enables additional automated input.

## Automated checks

Cargo tests cover all 18 type mappings, individual config switches, unchanged legacy config behavior, minimum movement-duration validation, run timer reset on interruptions, a movement gate that never blocks Life/Mana/other utility choices, and existing PoE2 checks. CI compilation and tests cannot validate the actual Run animation against the game; live validation remains required.
