# pxtone migration (stereo scope + VU capability)

Status: ready-for-agent
Type: AFK
Est. context: ~55k

## Parent

Tracker/Scope redesign PRD (proving set — stereo scope + VU).

## What to build

Migrate `playback-pxtone` to prove **stereo scope** and the separate **VU** capability. Advertise `caps = Scope | Vu`. Per scope channel declare `scope_width` (1 mono / 2 stereo) in its `RVChannelDesc`; `get_scope_data` returns interleaved samples at that width. Implement explicit `set_scope_enabled` (no hidden auto-on) and `get_vu` (cheap per-channel peak/RMS). Verify through the harness, including a stereo voice returning width-2 interleaved data and VU present.

Risk/decision note: pxtone's current scope is a mono per-Unit passthrough to its `moo_*` lib; proving real stereo + VU likely needs new capability in that lib layer, not just vtable plumbing. If pxtone's lib makes stereo/VU expensive, fall back to **v2m** (already has stereo-capable scope) as the stereo+VU proving plugin — the goal is to prove the contract once, on whichever is cheaper.

## Acceptance criteria

- [ ] Plugin advertises `Scope | Vu`; at least one scope channel reports `scope_width = 2` and returns interleaved stereo samples.
- [ ] `set_scope_enabled(false)` stops capture (no hidden auto-on); `set_scope_enabled(true)` resumes.
- [ ] `get_vu` returns per-channel levels when `Vu` is advertised.
- [ ] Harness test asserts stereo width, interleaving, the on/off switch, and VU presence.

## Blocked by

- #03 (committed dlopen viz harness).
