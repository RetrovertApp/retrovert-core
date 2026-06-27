# Rollout — ixalance migration (full pattern-cell tracker)

Status: ready-for-agent
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

- [ ] ixalance advertises the correct caps + scroll mode and a complete column schema.
- [ ] `get_cells(channel=-1, …)` returns a window of cells with both raw values and
      rendered fixed-width text matching the pattern of a known IXS module.
- [ ] Pattern + scope channel counts/names reported; scope verified non-silent via
      the harness, silent until `set_scope_enabled(true)`.
- [ ] ixalance no longer references the old tracker vtable slots (ready for #14).

## Blocked by

- #03 (committed dlopen viz harness).
