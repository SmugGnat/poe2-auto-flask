# PoE1 Milestone 2 — experimental auto-flask

Development branch: feature/poe1-auto-flask, based on feature/poe1-readonly-session. Does not modify main, develop, or the local UI work.

## What GameHelper does

AutoHotKeyTrigger exposes Flasks.Flask1 through Flasks.Flask5 and evaluates user-configured, slot-specific conditions. Its starter profile hard-codes two Life slots, a Quicksilver in slot 4, and a Mana flask in slot 5. It does not automatically select among duplicate flask types or discover their activation keys from the PoE1 production configuration.

## Selection and settings

Life and Mana use the existing health/mana percentages, calculated against the unreserved resource. Utility flask types are identified from item metadata, not the slot. Supported opt-in settings in config.toml:

    [poe1.utility]
    quicksilver = false
    granite = false
    quartz = false
    diamond = false

All four start disabled. Other utility types and Hybrid flasks are intentionally manual-only in this milestone. The PoE1 binding reader loads all five slots from Documents/My Games/Path of Exile/production_Config.ini, with no per-slot key configuration in this app. Missing, unsupported or duplicate binds block input.

Duplicates are selected by maximum whole uses remaining (current charges divided by effective cost), then highest current charges, then lowest slot number. If another flask of the same type has an active effect, the engine skips that type. Life and Mana take priority over optional utility rules.

Special recovery modifiers (e.g. instant recovery on low life), flask recovery amounts, enchantment auto-use, and rotational priorities are **not** modeled. The initial selection policy deliberately doesn't pretend that highest charges means optimal recovery.

## Effective charge cost

Following the reference GameHelper structure: PoE1's base Charges.PerUseCharge is multiplied by the player's flask_charges_used_positive_percentage stat (0x2E5) and matching explicit FlaskChargesUsed Mods, then truncated to an integer. When any required part is unreadable, effective cost is unavailable and that flask is ineligible for automatic use. The diagnostic continues reporting raw charges.

The per-item modifier cache is invalidated on item replacement / game-state refresh. Player charges-used stats are requested again when action state is read. Confirm the user's observed base 15 / effective 11 with the live client.

## Safety

PoE1 requires an explicit --auto or --debug argument in the Linux launcher, starts DISARMED, and uses F11 while the game is focused. Input is blocked when unfocused, dead, in a town or disabled hideout, during grace_period, when chat is open or unreadable, when keybindings or memory are invalid, or when insufficient charges/effects are active. A one-second post-use guard prevents repeated activations. All memory reads are read-only, with the existing normal Win32 input method for allowed actions. No injection, drivers, extra threads or dependencies.

The PoE2 automation loop and existing PoE2 configuration remain unchanged.

## Validation needed before merging

1. In PoE1 with the new development AppImage, run `--game poe1 --debug` and verify chat reports OPEN while typing and closed otherwise.
2. Confirm the modified Life Flask displays raw 15 and effective 11, and manually use it to confirm the observed charge delta.
3. Test changing zones and equipped flask slots.
4. Only then run `--game poe1 --auto`, intentionally arm with F11, and verify Life/Mana thresholds and a deliberately enabled utility toggle.
5. Confirm town/hideout, focus, chat, grace period, empty flask, and insufficient-charge blocking. Test a duplicate type and CI-like character.
6. Regression-test PoE2 automatic mode on Proton and Windows.

CI can validate the parser, selection rules, Rust 1.85 compatibility and release builds, but not game-memory offsets or live input behavior. Keep the PR as a draft until the live checks pass.
