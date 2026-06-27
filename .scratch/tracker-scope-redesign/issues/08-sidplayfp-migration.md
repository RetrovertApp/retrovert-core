# sidplayfp migration (metadata-only + scope, channel count > 8)

Status: done
Type: AFK
Est. context: ~35k

## Parent

Tracker/Scope redesign PRD (proving set — metadata-only model).

## What to build

Migrate `playback-sidplayfp` to prove the **metadata-only + scope** model: a plugin that provides a scope and catalog info but no pattern grid. Advertise `caps = Scope` (no `PatternCells`); leave the cell getters unimplemented/empty. Implement `get_structure`, scope channel descriptors, and the new `get_scope_data`. Exercise the **dynamic channel count exceeding the old `RV_MAX_CHANNELS = 8`** — a multi-SID tune reports up to 9 (3 SIDs × 3 voices); confirm the new dynamic-count API handles it cleanly. Verify through the harness.

## Acceptance criteria

- [x] sidplayfp advertises `Scope` only (no `PatternCells`); host gets no pattern grid, no crash.
- [x] Scope channel count is dynamic and correctly reports >8 for a multi-SID tune.
- [x] `get_scope_data` returns non-silent samples on a known file.
- [x] Harness test asserts metadata-only behaviour + the >8 channel count.

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- Commit (retrovert-core): viz: 08 migrate sidplayfp to the new viz vtable
- Plugin change (playback-sidplayfp `sidplayfp_plugin.cpp` + vendored ABI headers) left uncommitted in the working tree, mirroring the openmpt (#04) precedent.
- Test fixture: a 3-SID tune copied to `replay_frontend/.../music/sidplayfp/Devils_ReSIDence_3SID.sid` so the >8 channel assertion runs.
- Reviewers: 2 (combined correctness+quality, test-coverage)
- Rounds: 1, 0 blocking findings (1 non-blocking advisory, not applied)
