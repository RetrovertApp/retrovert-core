# Tests: ring-buffer math + headless pipeline

Status: done
Type: AFK
Est. context: ~80k

## What to build

Cover the riskiest, currently-untested logic. Unit tests for the ring-buffer wrap math in `playback.rs`: `Index` generation bumps and `get_byte_size_format` (trivial/pure), plus `get_data` + `copy_buffer_to_ring` (wrap-around). The latter two need an in-file `#[cfg(test)]` seam and a fabricated no-op `ResamplePluginInstance` (since `PlaybackInternal::new` bails without a resampler).

One headless integration test drives all 3 threads with a fake decoder: hand-write `extern "C"` vtables for a `PlaybackPlugin` returning canned PCM and an identity `ResamplePlugin`, queue playback, and assert frames flow through. Reuse the shape of the existing `core/tests/viz_*.rs` harness. No audio device.

## Acceptance criteria

- [x] Unit tests for `Index`, `get_byte_size_format`, `get_data`, `copy_buffer_to_ring` (incl. ring-buffer wrap).
- [x] One headless integration test drives the decode→ring→get_data path with fake vtables and asserts frame flow.
- [x] `cargo test` is green and needs no audio device or external plugin.

## Blocked by

- 01 (prefactor) — does not require cpal/playback to be working.

## Comments

### 2026-06-28 — Implemented
- Reviewers: 2 (combined correctness+quality, test coverage)
- Rounds: 1, 0 findings across the loop
- Note: tests live in-file under `#[cfg(test)]` in `playback.rs` (not `core/tests/`) — the registry path needs a real `libloading::Library`, and `#[cfg(test)]` seams are invisible to integration binaries; the viz-harness technique is reused.
- Deferred (non-gating, out of scope): the `format != DEFAULT_AUDIO_FORMAT` resampler convert/wrap path in `get_data` is still untested.
