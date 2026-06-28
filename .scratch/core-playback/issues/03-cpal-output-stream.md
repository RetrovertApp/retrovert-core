# cpal audio output in-core (stream up)

Status: done
Type: AFK
Est. context: ~75k

## What to build

Replace the output-plugin path with a Rust `cpal` output module inside core (per ADR-0004). Open a cpal output stream and feed it from the decode thread via the existing `PlaybackMessage::GetData` round-trip (keep the round-trip for now — ADR-0004 defers the lock-free SPSC optimization). Handle device sample-rate/format negotiation against the internal `DEFAULT_AUDIO_FORMAT` (48k / f32 / stereo), configuring the output resampler accordingly.

Prove an audible stream — a generated test tone or the decoder loaded in slice 02. Single-file correctness is slice 04; the buggy randomize path may still be present here. Drop the C output plugin from the output path; the in-house miniaudio bindings are not used.

## Acceptance criteria

- [x] `output.rs` opens a cpal stream and pulls audio from the decode thread.
- [x] Device rate/format mismatch with the internal format is handled via the resampler.
- [x] Audible output is produced (test tone or a loaded decoder) without continuous underruns.
- [x] No dependency on an output plugin or the miniaudio Rust bindings.

## Blocked by

- 01 (prefactor)
- 02 (a loadable decoder to feed real data; a test tone otherwise)

## Comments

### 2026-06-28 — Implemented
- Commit: 17e483a core: 03 cpal audio output in-core
- Reviewers: 1 (combined correctness+quality+coverage)
- Rounds: 2, 2 findings (both low/quality: removed an ADR-0004 ref and two multi-line explanatory comments per the no-verbose-comments style)
- Notes: `output.rs` now opens a `cpal` output stream (F32/I16, device-negotiated rate/channels); the audio-thread callback does the existing `PlaybackMessage::GetData` round-trip and fills silence on a miss. The device `AudioFormat` is handed to the decode thread, whose `output_resampler` converts the internal 48k/f32/stereo. C output-plugin path deleted from `output.rs` (`Output::new` drops the `output_plugins` arg); the output-plugin ABI stays loaded-but-dormant per ADR-0004. Added `cpal = "0.15"`. Audible verification is a manual run (headless CI has no audio device).
