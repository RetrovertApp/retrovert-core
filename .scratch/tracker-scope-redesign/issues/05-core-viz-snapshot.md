# Core decode-thread visualization snapshot + UI handoff

Status: done
Type: AFK
Est. context: ~60k

## Parent

Tracker/Scope redesign PRD (threading / snapshot seam). The core consumes ZERO viz getters today — this is net-new.

## What to build

Make the core call the new visualization getters **on the decode thread**, interleaved with `read_data`, copy the results into a single coherent **value-semantic snapshot** stamped with the output-frame position, and hand that snapshot to the UI thread. Because the ABI is value-semantic (ADR-0002), building the snapshot is a plain struct/buffer copy — no borrowed-pointer lifetime tracking. Add the snapshot type and the request/reply (or shared-buffer) path alongside the existing crossbeam `PlaybackMessage`/`PlaybackReply` plumbing in `core/src/playback.rs`; put the snapshot type in a new `core/src/visualization.rs`.

Test the snapshot seam without a UI and without depending on a specific real plugin: drive the core against a hand-written **stub vtable** implementing the new viz slots, and assert the produced snapshot's fields (caps, position incl. `output_frame`, a window of cells, channel names, scope) match expectation.

## Acceptance criteria

- [x] Core calls the viz getters on the decode thread (same thread as `read_data`), not the UI thread.
- [x] A frame-stamped snapshot (carrying `output_frame`) is built by value and made available to a consumer thread.
- [x] Snapshot fields match a known stub plugin's outputs in a test (no real UI, no dlopen required).
- [x] No data races: the UI side never touches plugin state directly.

## Blocked by

- #02 (Rust bindings for the new viz vtable). Uses the #03 harness scaffolding where convenient but can run in parallel with #04 (tests via a stub vtable, not openmpt).

## Comments

### 2026-06-27 — Implemented
- Commit: viz: 05 decode-thread snapshot + UI handoff
- Reviewers: 2 (combined correctness+quality, test coverage)
- Rounds: 2, 5 findings across the loop (2 gating fixed, 3 coverage notes closed)
