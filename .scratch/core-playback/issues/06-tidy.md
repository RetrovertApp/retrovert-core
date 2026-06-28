# Tidy: dead code, message enums, playlist modes

Status: done
Type: AFK
Est. context: ~55k

## What to build

With a running, tested player as the safety net, clean up the core. Remove the ~55 `dbg!`/`unwrap`/TODO sites across `playback.rs`/`playlist.rs`/`output.rs` (notably `dbg!` in the decode hot loop). Tidy the message enums across the 3 threads (drop dead variants; keep the 3-thread crossbeam architecture — the RT round-trip is a deliberate, documented ceiling). Make the existing playlist `Mode` handling (Default/Randomize, AddUrl/PlayUrl) coherent.

Scope guard: do **not** add new features (e.g. implementing the commented-out `NextSong`) — tidy only.

## Acceptance criteria

- [x] No `dbg!` in hot paths; `unwrap`s on fallible runtime paths replaced with error handling/logging.
- [x] Message enums contain no dead variants; the 3-thread design is intact.
- [x] Playlist modes are coherent and documented in code where non-obvious.
- [x] Player still plays a file and `cargo test` stays green.

## Blocked by

- 04 (single-file playback) — touches the output/message paths
- 02 (decoder available)

## Comments

### 2026-06-28 — Implemented
- Reviewers: 1 (combined correctness + quality + test coverage)
- Rounds: 1, 0 findings
- Note: `AddUrl`/`add_url` was kept (not deleted) per maintainer call and made
  coherent — it enqueues without setting the randomize anchor that `PlayUrl`
  sets. Plugin C-ABI fn-pointer `.unwrap()`s left intact (contract asserts, not
  runtime-fallible; out of tidy scope).
