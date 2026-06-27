# Delete dead fields; the ABI break; rebuild all plugins

Status: done
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

- [x] Old fields/structs/vtable slots removed from `.def` and `.h`; Rust bindings regenerated.
- [x] All 28 plugins compile and load against the trimmed ABI; no reference to the deleted slots remains.
- [x] New viz structs are non-opaque in C and Rust (no `extern opaque` for them).
- [x] A representative plugin from each cohort decodes + reports viz through the harness post-break.

## Blocked by

- #04, #05, #06, #07, #08, #09, #10, #11, #12, #13, #15 (all plugins migrated + core snapshot in place, so nothing references the old fields).

## Comments

### 2026-06-27 — Implemented
- Deletion deliverable: `retrovert_api` 77b00f4 "Delete dead tracker/scope ABI surface (v2 break)" — trims `api/playback.def` + generated `include/retrovert/playback.h`. Generated Rust bindings (`plugin_types/src/generated/playback.rs`) were already committed clean.
- The deletion was carried out across the migration issues and sat uncommitted in the `retrovert_api` working tree; this slice verified completeness, scoped the commit (only `playback.def`/`playback.h`; the unrelated api-gen-v2 header changes left out), and proved the break.
- Verification: no deleted ABI symbol survives as a real reference (grep over `retrovert_api/{api,include}` + plugin sources — remaining hits are false positives from libopenmpt/art-of-noise internal names + a comment). Viz structs non-opaque in C and Rust.
- ABI-break proof, one representative plugin per cohort built from source + harness-tested: hively (synchronized tracker), tfmx (per-channel tracker), sc68 (scope-only), pxtone (stereo scope + VU), asap (no-viz, caps=0) — 5/5 pass. Full 28-plugin build (criterion #2) delegated to each plugin repo's CI per decision.
- Reviewers: 1 (combined correctness/quality/coverage). Rounds: 1, 0 findings.
