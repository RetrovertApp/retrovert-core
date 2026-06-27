# Rollout — tracker stragglers (hively, ixalance, art-of-noise)

Status: ready-for-agent
Type: AFK
Est. context: ~90k

## Parent

Tracker/Scope redesign PRD (staged rollout — stage that the PRD wrongly lumped into "mechanical"; reviewers found these are full pattern-cell migrations).

## What to build

Migrate the three remaining **full pattern-cell tracker** plugins to the new viz vtable — the same windowed-cells work as the proving set, NOT mechanical scope-only stubs:

- `playback-hively` (AHX) — native random-access, synchronized (like openmpt).
- `playback-ixalance` — pattern-cell tracker; ~181 viz lines embedded in a very large (~17k-line) source file — budget orientation cost accordingly; consider carving ixalance into its own slice if it runs hot.
- `playback-art-of-noise` — verify during implementation whether it implements pattern cells (reviewers disagreed); if it does, migrate as a tracker; if not, move it to a scope-only batch.

Each: advertise correct caps + scroll mode, implement structure/columns/channels/position/`get_cells`/scope, standardize effect encoding into `RVCell`. Verify each through the committed harness.

## Acceptance criteria

- [ ] hively migrated: synchronized cells with raw + rendered text verified on a known AHX module via the harness.
- [ ] ixalance migrated: pattern cells + scope verified via the harness.
- [ ] art-of-noise classification confirmed and migrated to the matching model (tracker or scope-only).
- [ ] None of these three reference the old tracker vtable slots afterward (ready for the #14 deletion).

## Blocked by

- #03 (committed dlopen viz harness).
