# pxtone migration (stereo scope + VU capability)

Status: done
Type: AFK
Est. context: ~55k

## Parent

Tracker/Scope redesign PRD (proving set — stereo scope + VU).

## What to build

Migrate `playback-pxtone` to prove **stereo scope** and the separate **VU** capability. Advertise `caps = Scope | Vu`. Per scope channel declare `scope_width` (1 mono / 2 stereo) in its `RVChannelDesc`; `get_scope_data` returns interleaved samples at that width. Implement explicit `set_scope_enabled` (no hidden auto-on) and `get_vu` (cheap per-channel peak/RMS). Verify through the harness, including a stereo voice returning width-2 interleaved data and VU present.

Risk/decision note: pxtone's current scope is a mono per-Unit passthrough to its `moo_*` lib; proving real stereo + VU likely needs new capability in that lib layer, not just vtable plumbing. If pxtone's lib makes stereo/VU expensive, fall back to **v2m** (already has stereo-capable scope) as the stereo+VU proving plugin — the goal is to prove the contract once, on whichever is cheaper.

## Acceptance criteria

- [x] Plugin advertises `Scope | Vu`; at least one scope channel reports `scope_width = 2` and returns interleaved stereo samples.
- [x] `set_scope_enabled(false)` stops capture (no hidden auto-on); `set_scope_enabled(true)` resumes.
- [x] `get_vu` returns per-channel levels when `Vu` is advertised.
- [x] Harness test asserts stereo width, interleaving, the on/off switch, and VU presence.

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- Commits: playback-pxtone `viz: migrate to v2 vtable (stereo scope + VU)`; retrovert-core `viz: 09 migrate pxtone to the new viz vtable`
- All scope channels report `scope_width = 2`; the moo scope-capture patch now keeps per-unit L/R separate (was collapsed to mono) and `moo_get_scope_data` returns interleaved samples. VU is the per-channel peak of the scope ring, so no extra lib code. pxtone proved cheap enough for real stereo — no v2m fallback.
- Reviewers: 2 (combined correctness+quality, test coverage)
- Rounds: 1, 0 findings across the loop
- Note: the core decode-thread snapshot (`build_snapshot`) sizes the VU buffer by `pattern_channel_count`, which is 0 for a scope-only plugin, so production VU would currently come back empty for pxtone. The harness exercises `get_vu` directly and passes. Surfacing as a possible core (#05) follow-up — out of scope for this issue.
