# Profiles and settings

The production UI and automation backend share the Rust `ProfileStore`, typed
validators and user-editable TOML files. No separate UI settings database is used.

## Storage layout

| Platform | Root |
| --- | --- |
| Windows | `%APPDATA%\poe2-auto-flask\` |
| Linux | `$XDG_CONFIG_HOME/poe2-auto-flask/` or `~/.config/poe2-auto-flask/` |

```text
poe2-auto-flask/
  settings.toml
  profiles/
    poe1-default.toml
    poe2-default.toml
    poe1-mapping.toml
  config.toml              # retained only if migrated from a legacy installation
```

Global `settings.toml` selects an independent active profile for each game:

```toml
schema_version = 1
active_poe1 = "poe1-default"
active_poe2 = "poe2-default"
automation_key = "F11"

[ui]
hide_to_tray_on_close = true

[ui.window]
x = 120
y = 160
width = 780
height = 552

[ui.skills]
molten_shell_expanded = false
blood_rage_expanded = true
spirit_offering_expanded = false
```

`automation_key` may be omitted for F11. It accepts one letter, digit or
F1–F12. The optional `[ui.window]` stores normal physical size and position;
`x`/`y` may be absent where placement cannot be queried. No window table is
needed before a placement has been saved. `[ui.skills]` remembers expanded
sections across restarts; all sections default to expanded when omitted.
Geometry, tray and section-layout changes do not reload automation rules or
disable automation. Shortcut changes do disable it.

A PoE1 profile (optional settings omitted):

```toml
schema_version = 1

[profile]
id = "poe1-mapping"
name = "Mapping"
game = "poe1"

[general]
enable_in_hideout = false

[health]
enabled = true
threshold_percent = 60.0

[mana]
enabled = true
threshold_percent = 30.0
```

The filename stem and `profile.id` must match. Rename changes only the display
name. PoE2 profiles cannot carry nondefault PoE1-specific rules. Automation
fields are documented in [PoE1 support](POE1_SUPPORT.md) and
[`config.example.toml`](../config.example.toml); the latter is a legacy
configuration example, not a complete profile.

## Migration and defaults

On first startup, the store validates the root's existing `config.toml`, or
an adjacent executable's legacy file if no root copy exists. It creates the
PoE1/PoE2 defaults from those settings, removing PoE1-specific rules from the
PoE2 copy, then writes `settings.toml`. The original file is never modified
or deleted. Once migrated, `config.toml` is not live configuration.

Without a legacy file, backend defaults apply: Life enabled at 60%, Mana
enabled at 30%, hideouts off and PoE1 utility/unique/skill automation off.
The overall automation state still starts disabled. New profiles and Reset
use those same defaults. Reset preserves profile identity and global settings.

Migration interrupted between writes resumes only if existing default
profiles match the validated source. Unknown schema versions, corrupt/missing
active profiles and conflicting migration files fail closed.

The UI imports a previous `window-state.toml` only if no valid placement is
already present in settings. It removes the old file after a successful atomic
settings save; a failed migration leaves the original placement intact.

## Save, reload and conflict handling

The UI keeps a `ProfileEdit` with an opaque file revision while settings are
being edited. Save validates and rejects stale revisions; Discard restores
the stored profile. Unsupported/unexposed fields survive saves. New profiles
use exclusive creation; replacements use a same-directory temporary file,
flush-to-disk and atomic replacement. Profile IDs permit only lowercase ASCII
letters, numbers, hyphen and underscore to prevent path traversal.

The editor checks settings/profile metadata every two seconds while visible.
External changes reload only when no settings draft or profile operation is
in progress. Hidden/minimized refresh stops and resumes on restoration.
Saving placement acknowledges only the settings write, preserving detection
of unrelated external profile edits. An external settings change is not
silently overwritten by a placement save.

The backend checks only global settings and the active game's profile once
per second, using its existing configuration poll. A profile switch disables
automation, rebuilds rules and resets activation state. UI-only settings do
not change automation configuration. Directory enumeration belongs to the
editor's profile listing, outside the automation hot path.

Changing the global shortcut reserves its base key against enabled PoE1 skill
bindings and requires a fresh foreground press before automation resumes.
The configured skill keys must match the game's actual bindings.

## Proton and overrides

The AppImage maps the host XDG profile root through Wine's Z: drive using
`POE2_AUTO_FLASK_PROFILE_DIR`. Manual GUI launches can select a root with
`--config-dir PATH` (highest priority) or that environment variable. A Unix
path is mapped to Wine's Z: path. If inherited HOME/XDG is unavailable under
Proton, pass the path explicitly instead of creating a second prefix store.

The normal GUI always uses profiles. The root development console binary
and packaged `--debug` path without `--config-dir` also support
`POE2_AUTO_FLASK_CONFIG` as an explicit legacy single-file override. It
selects that file without migration. It does not redirect the normal GUI's
profile store; use `--config-dir` for that. The Linux launcher preserves this
override for diagnostic/legacy commands and otherwise supplies the shared
profile root.
