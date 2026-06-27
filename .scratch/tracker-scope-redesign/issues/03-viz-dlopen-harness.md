# Committed dlopen visualization test harness + minimal RVService

Status: done
Type: AFK
Est. context: ~45k

## Parent

Tracker/Scope redesign PRD (Testing Decisions — primary seam).

## What to build

A **committed**, reusable test harness that loads a real compiled playback plugin via `dlopen`, provides a minimal real `RVService` (io/log/settings/metadata), drives the new visualization vtable after `open`, and exposes the results to assertions. This generalizes the throwaway issue-05 load-and-play harness into the shared seam every proving/rollout slice tests against. Reuse the existing `core/src/plugin_handler.rs` loading + service pattern.

The harness must be able to: build/load a plugin `.so`, call `get_structure`/`get_columns`/`get_pattern_channels`/`get_scope_channels`/`get_position`/`get_channel_rows`/`get_cells`/`set_scope_enabled`/`get_scope_data`/`get_vu` with caller-owned buffers sized from the structure counts, and return the typed values to the test. Include the `[slice_mut(T)]` caller-buffer plumbing once, here, so plugin slices don't each reinvent it. Drive it against a tiny in-test stub vtable (so the harness is verifiable without a migrated plugin yet).

## Acceptance criteria

- [x] Harness `dlopen`s a plugin and supplies a working minimal `RVService`; `open` succeeds. (`core/tests/viz_openmpt.rs`)
- [x] Each new viz getter is callable through the harness with correctly-sized caller buffers; counts/typed structs come back.
- [ ] A self-test drives a stub vtable and asserts the round-tripped values. → folded into #05 (its snapshot test drives a stub vtable; the committed harness drives the real openmpt `.so`, which proves the same plumbing end to end).
- [x] Committed to the repo (not throwaway); reusable scaffolding for later slices.

## Blocked by

- #02 (new viz ABI authored + Rust bindings regenerated).
