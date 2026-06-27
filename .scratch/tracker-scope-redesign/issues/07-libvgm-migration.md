# libvgm migration (synthesized + windowed; delete native_pattern_data)

Status: done
Type: AFK
Est. context: ~79k

## Parent

Tracker/Scope redesign PRD (proving set — synthesized-from-event-stream model; the only user of `native_pattern_data`).

## What to build

Migrate `playback-libvgm` to prove the **synthesized + windowed** model and to remove the dead `native_pattern_data` escape hatch entirely. libvgm parses the whole register-write stream at `open`, so it can expose a window covering everything and advertise `WholeSongKnown`, but it must serve cells through the standard windowed `get_cells` from its post-open synthesized data — not via a private pointer. Implement the full pattern + scope vtable; standardize effect encoding into `RVCell`. This is the hairiest proving plugin: ~222 viz lines today, a `HAS_VGM_PATTERN` conditional-build flag, and large `external/` deps — budget build/iteration friction accordingly. Verify through the harness with both build configs sane.

## Acceptance criteria

- [x] `native_pattern_data` is gone from libvgm; cells are served via `get_cells` from synthesized data.
- [x] libvgm advertises per-channel scrolling + `WholeSongKnown`; windowed `get_cells` returns correct raw + text.
- [x] Scope works through the new `get_scope_data`.
- [x] Builds and passes the harness test in both `HAS_VGM_PATTERN` configurations.

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- Plugin migration (libvgm_plugin.cpp + CMakeLists `LIBVGM_VGM_PATTERN` option) left uncommitted in the playback-libvgm working tree, matching the #04/#06 rollout. Committed here: the `core/tests/viz_libvgm.rs` harness test.
- libvgm now advertises PerChannel + WholeSongKnown, serves windowed `get_cells` from the synthesized `VgmPattern` (native_pattern_data deleted), standardizes effect encoding, and exposes scope via `get_scope_samples`. Verified against both `HAS_VGM_PATTERN` build configs.
- Reviewers: 3 (combined correctness+quality, test-coverage, whole-file). Rounds: 2, 5 findings across the loop (3 fixed: orphaned debug statics, scope auto-enable mirror flag, get_vu doc; 2 test/coverage notes folded in). 4 pre-existing structural items in libvgm_plugin.cpp deferred to a follow-on cleanup pass.
