# Delete dead fields; the ABI break; rebuild all plugins

Status: ready-for-agent
Type: AFK
Est. context: ~100k (borderline — may batch the 28 initializer edits per cohort)

## Parent

Tracker/Scope redesign PRD (final stage — deletions). Closes the single-source-codegen and dead-field goals.

## What to build

Now that every plugin is on the new viz vtable, remove the old surface — the deliberate ABI break:

- Delete from `playback.def` + the hand-authored `playback.h`: `native_pattern_data`, `sample_names[32][24]`, `RV_MAX_CHANNELS`, and the old vtable slots (`get_tracker_info`, `get_pattern_cell`, `get_pattern_num_rows`, the old `get_scope_data`, `get_scope_channel_names`) plus `RVTrackerInfo`/`RVChannelInfo`/old `RVPatternCell` if unused.
- Regenerate the Rust bindings and fan the header out to all 28 plugins.
- Fix every plugin's positional struct initializer (removing the old slots shifts positions — all 28 need an edit + rebuild). To keep this clean, plugins should have been converted to designated initializers during their migration; if not, that conversion is part of this slice.
- Confirm the new viz structs are fully **non-opaque** in both the hand-authored C header and the generated Rust.

If the 28-plugin initializer edits + rebuilds push this past budget, split the deletion+rebuild per cohort (mirroring the rollout batches) behind a single `.def`/`.h` deletion.

## Acceptance criteria

- [ ] Old fields/structs/vtable slots removed from `.def` and `.h`; Rust bindings regenerated.
- [ ] All 28 plugins compile and load against the trimmed ABI; no reference to the deleted slots remains.
- [ ] New viz structs are non-opaque in C and Rust (no `extern opaque` for them).
- [ ] A representative plugin from each cohort decodes + reports viz through the harness post-break.

## Blocked by

- #04, #05, #06, #07, #08, #09, #10, #11, #12, #13 (all plugins migrated + core snapshot in place, so nothing references the old fields).
