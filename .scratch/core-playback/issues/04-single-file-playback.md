# Single-file end-to-end playback

Status: done
Type: AFK (human spot-check on audio)
Est. context: ~45k

## What to build

Make `retrovert-console --play <file>` decode and play exactly one real module end-to-end out the speakers. Fix `PlayUrl` so it plays a single file instead of forcing randomize mode and double-pushing the url (`incoming_msg` in `playlist.rs`). Final wiring so file in → decode → ring buffer → cpal → sound, then ends cleanly.

This is the tracer-bullet payoff.

## Acceptance criteria

- [ ] `retrovert-console --play <file>` plays one real module and it is audible.
- [x] `PlayUrl` plays a single file (no forced randomize, no double-push).
- [x] Playback ends cleanly at end-of-song without panic.

## Blocked by

- 03 (cpal output stream)
- 02 (decoder)

## Comments

### 2026-06-28 — Implemented
- Commit: core: 04 single-file end-to-end playback (see git log)
- Reviewers: 1 (combined-all)
- Rounds: 1, 0 findings across the loop
- Code path (file → playlist → playback queue → ring buffer → cpal → clean
  exit) is in place and approved. Criterion 1 (audible) left unticked: no
  decoder/resample plugins are built in this checkout, so audible playback is
  the remaining human spot-check (run `retrovert-console --play <file>` with
  built plugins). Code-verifiable criteria 2 and 3 are ticked.
