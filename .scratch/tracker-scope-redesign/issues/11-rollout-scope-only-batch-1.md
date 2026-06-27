# Rollout — scope-only batch 1

Status: done
Type: AFK
Est. context: ~110k

## Parent

Tracker/Scope redesign PRD (staged mechanical rollout).

## What to build

Migrate the first batch of **scope-only** plugins to the new viz API: advertise `caps = Scope` (add `Vu` only where cheaply available), implement `get_structure`, scope channel descriptors (with `scope_width`), explicit `set_scope_enabled`, and the new `get_scope_data`. No pattern cells. This is largely repetitive plumbing per plugin, but each still needs a from-source build and a harness check.

Batch (≈6, confirm each plugin's actual capability while migrating; if one turns out to have no real scope, give it `caps = 0` and move it to the no-viz batch):

- adplug, audio_stream, furnace, klystrack, mdxmini, pmdmini

## Acceptance criteria

- [x] Each plugin advertises correct caps and implements the new scope getters (or `caps = 0` if it has no real visualization).
- [x] Each builds from source and returns non-silent scope (where applicable) through the harness.
- [x] No plugin in the batch references the old tracker/scope vtable slots afterward.

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- All 6 migrated as scope-only (caps = Scope; no plugin needed caps = 0):
  - adplug (9 OPL ch), audio_stream (1–2 decoder ch), furnace (per-channel osc),
    klystrack (n_channels from KSND_GetSongInfo), mdxmini (8 YM2151 FM),
    pmdmini (6 FM + 3 SSG).
- Each: get_structure + get_scope_channels (scope_width = 0) + explicit set_scope_enabled
  (auto-on removed) + get_scope_samples; all pattern getters and get_vu left NULL.
  Vu skipped everywhere (host derives VU from scope per PRD; klystrack's raw envelope
  meters have no clean scale).
- furnace got a host-facing scope_enabled flag (engine captures unconditionally).
- Latent link bugs fixed: audio_stream + klystrack now link libm (Windows-guarded) —
  stb_vorbis/klystron reference log/pow, exposed by the dlopen harness.
- Added 6 dlopen harness tests (core/tests/viz_*.rs) asserting caps, NULL pattern/vu
  getters, scope channel count/names, silence-before-enable, non-silence-after-enable.
- Commits: 6 plugin repos (code + CMakeLists + new-ABI header sync) + retrovert-core
  (harness tests + this tracker update). Each repo's pre-existing build.yml CI tweak
  left uncommitted (unrelated to viz).
- Reviewers: 3 (correctness, code-quality, test-coverage).
- Rounds: 1, 0 blocking findings (3 non-blocking test-hardening notes from coverage,
  applied: assert scope_width == 0 and get_vu is NULL).
