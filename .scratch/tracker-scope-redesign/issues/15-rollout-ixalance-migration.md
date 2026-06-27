# Rollout — ixalance migration (full pattern-cell tracker)

Status: done
Type: AFK
Est. context: ~90k

## Parent

Tracker/Scope redesign PRD (staged rollout). Carved out of #10: ixalance is a
full pattern-cell tracker whose viz code (~348 lines: IT-pattern decompression +
tracker + scope) is embedded in a very large (~17k-line) source tree, so it gets
its own slice to keep the migration inside one context budget.

## What to build

Migrate `playback-ixalance` to the new viz vtable — the same windowed-cells +
scope work as the proving set (openmpt #04, pxtone #09, hively/aon #10), NOT a
mechanical scope-only stub:

- Advertise correct caps + scroll mode (synchronized random-access, like openmpt).
- Implement structure/columns/pattern+scope channels/position/`get_channel_rows`/
  `get_cells` (windowed batch; `channel = -1` fills all channels row-major) and the
  new `set_scope_enabled`/`get_scope_samples`/`get_vu`.
- Each cell carries the raw value plus fixed-width rendered text; standardize the
  effect encoding at the plugin boundary (raw = command byte, text = rendering).
- Reuse the existing `IxsScopeCapture` for `get_scope_samples`; gate it strictly on
  `set_scope_enabled` (silent until enabled).
- Convert the plugin struct to the new tail and remove the old tracker/scope slots
  (`get_tracker_info`, `get_pattern_cell`, `get_pattern_num_rows`, old
  `get_scope_data`, `get_scope_channel_names`).
- Verify through the committed dlopen harness (`core/tests/viz_ixalance.rs`) against
  a known IXS module (modland `Ixalance/Crystal Score/*.ixs`).

## Acceptance criteria

- [x] ixalance advertises the correct caps + scroll mode and a complete column schema.
- [x] `get_cells(channel=-1, …)` returns a window of cells with both raw values and
      rendered fixed-width text matching the pattern of a known IXS module.
- [x] Pattern + scope channel counts/names reported; scope verified non-silent via
      the harness, silent until `set_scope_enabled(true)`.
- [x] ixalance no longer references the old tracker vtable slots (ready for #14).

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- Commit: retrovert-core "viz: 15 migrate ixalance to the new viz vtable"
- Plugin source: `playback-ixalance/ixalance_plugin.cpp` migrated in working tree
  (committed in bulk at the ABI break, #14, like the other cohorts).
- Reviewers: 2 (correctness+quality, test-coverage)
- Rounds: 2, 4 findings across the loop (1 med + 2 low test-coverage gaps; 1
  non-blocking correctness note — all fixed)
- Verified: real IXS fixture (`Ixalance/Crystal Score/andes.ixs`) at the harness
  default path; `cargo test -p rv_core --test viz_ixalance` passes.
