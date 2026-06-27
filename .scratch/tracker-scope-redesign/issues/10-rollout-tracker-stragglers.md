# Rollout — tracker stragglers (hively, art-of-noise)

Status: done
Type: AFK
Est. context: ~90k

## Parent

Tracker/Scope redesign PRD (staged rollout — stage that the PRD wrongly lumped into "mechanical"; reviewers found these are full pattern-cell migrations).

ixalance was carved out into #15: its viz code (~348 lines) is embedded in a ~17k-line source tree and runs hot enough to need its own context budget.

## What to build

Migrate the remaining **full pattern-cell tracker** plugins to the new viz vtable — the same windowed-cells work as the proving set, NOT mechanical scope-only stubs:

- `playback-hively` (AHX/HVL) — native random-access, synchronized (like openmpt).
- `playback-art-of-noise` — verify during implementation whether it implements pattern cells (reviewers disagreed); if it does, migrate as a tracker; if not, move it to a scope-only batch.

Each: advertise correct caps + scroll mode, implement structure/columns/channels/position/`get_cells`/scope, standardize effect encoding into `RVCell`. Verify each through the committed harness.

## Acceptance criteria

- [x] hively migrated: synchronized cells with raw + rendered text verified on a known AHX module via the harness.
- [x] art-of-noise classification confirmed and migrated to the matching model (tracker or scope-only).
- [x] Neither plugin references the old tracker vtable slots afterward (ready for the #14 deletion).

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- Scope: ixalance carved out into #15 mid-flight (confirmed with user); this slice landed hively + art-of-noise.
- hively: migrated to the new viz vtable as a Synchronized random-access tracker (PatternCells | Scope | WholeSongKnown). Added real per-voice scope capture to hvl_replay (vc_ScopeBuf ring tapped in hvl_mixchunk, gated by ht_ScopeEnabled) — a deliberate superset of the AC, confirmed with user. 6-column schema (Note/Inst/FX/Prm/FX2/Pr2) surfaces both HVL effect slots. Verified on AHX (2_much_pressure.ahx) and HVL (adrenalyn.hvl).
- art-of-noise: confirmed a pattern-cell tracker (aon_song_get_pattern_cell) and migrated as one; 5-column schema (Note/Inst/Arp/Eff/Prm), rewired its native scope with strict set_scope_enabled gating. Verified on AON4 (shadow zone.aon).
- Commits: hively + art-of-noise plugin repos (code + new-ABI header sync), retrovert-core (harness tests + this tracker update).
- Reviewers: 4 (correctness, code-quality, test-coverage, engine-fidelity).
- Rounds: 2, 8 findings (all from test-coverage round 1; resolved by strengthening both harness tests). Correctness/quality/fidelity approved round 1.
