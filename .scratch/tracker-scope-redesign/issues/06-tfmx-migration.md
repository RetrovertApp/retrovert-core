# tfmx migration (per-channel non-synchronized scrolling)

Status: ready-for-agent
Type: AFK
Est. context: ~56k

## Parent

Tracker/Scope redesign PRD (proving set — per-channel model).

## What to build

Migrate `playback-tfmx` to prove the **per-channel non-synchronized** scrolling model. Advertise `scroll_mode = PerChannel`. Implement `get_channel_rows` (each channel reports its own current row), per-channel windowed `get_cells` (one call per channel over that channel's window), the column schema, channel descriptors, and scope. Standardize tfmx's mixed effect encoding (letter-for-command vs raw) into the `RVCell` raw value + rendered text at the plugin boundary. Preserve the per-channel routing info (old `dest_channel`) via the cell raw value so the host can colour it. Build from source and verify through the harness.

## Acceptance criteria

- [ ] tfmx advertises `PerChannel` scroll mode; `get_channel_rows` returns independent per-channel rows.
- [ ] `get_cells` per channel returns windowed cells with consistent raw + rendered text (no letter/raw inconsistency leaking to the host).
- [ ] Scope works through the new `get_scope_data`.
- [ ] Harness test asserts per-channel scrolling + cell contents on a known TFMX module.

## Blocked by

- #03 (committed dlopen viz harness).
