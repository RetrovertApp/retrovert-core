# libvgm migration (synthesized + windowed; delete native_pattern_data)

Status: ready-for-agent
Type: AFK
Est. context: ~79k

## Parent

Tracker/Scope redesign PRD (proving set — synthesized-from-event-stream model; the only user of `native_pattern_data`).

## What to build

Migrate `playback-libvgm` to prove the **synthesized + windowed** model and to remove the dead `native_pattern_data` escape hatch entirely. libvgm parses the whole register-write stream at `open`, so it can expose a window covering everything and advertise `WholeSongKnown`, but it must serve cells through the standard windowed `get_cells` from its post-open synthesized data — not via a private pointer. Implement the full pattern + scope vtable; standardize effect encoding into `RVCell`. This is the hairiest proving plugin: ~222 viz lines today, a `HAS_VGM_PATTERN` conditional-build flag, and large `external/` deps — budget build/iteration friction accordingly. Verify through the harness with both build configs sane.

## Acceptance criteria

- [ ] `native_pattern_data` is gone from libvgm; cells are served via `get_cells` from synthesized data.
- [ ] libvgm advertises per-channel scrolling + `WholeSongKnown`; windowed `get_cells` returns correct raw + text.
- [ ] Scope works through the new `get_scope_data`.
- [ ] Builds and passes the harness test in both `HAS_VGM_PATTERN` configurations.

## Blocked by

- #03 (committed dlopen viz harness).
