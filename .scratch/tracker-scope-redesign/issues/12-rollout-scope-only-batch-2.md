# Rollout — scope-only batch 2

Status: done
Type: AFK
Est. context: ~95k

## Parent

Tracker/Scope redesign PRD (staged mechanical rollout).

## What to build

Second batch of **scope-only** plugin migrations, same shape as batch 1 (issue #11): advertise `caps = Scope` (+`Vu` where cheap), implement `get_structure`, scope channel descriptors with `scope_width`, `set_scope_enabled`, new `get_scope_data`; no pattern cells; build from source and verify through the harness.

Batch (≈4, confirm each plugin's actual capability while migrating):

- sc68, spu, uade, v2m

(If v2m was used as the stereo+VU proving plugin in #09, drop it from here.)

## Acceptance criteria

- [x] Each plugin advertises correct caps and implements the new scope getters (or `caps = 0` if no real visualization).
- [x] Each builds from source and returns non-silent scope (where applicable) through the harness.
- [x] No plugin in the batch references the old tracker/scope vtable slots afterward.

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- All 4 migrated as scope-only (caps = Scope; none needed caps = 0). v2m stays in this
  batch — #09 proved pxtone cheap enough, no v2m fallback was used.
- Each: get_structure + get_scope_channels (scope_width = 0) + explicit set_scope_enabled
  (old hidden auto-on removed) + get_scope_samples; all pattern getters and get_vu NULL.
  - spu: 1 mono "ADPCM" channel; ring captured in read_data only when enabled; added
    missing `#include <stdio.h>` for snprintf.
  - uade: 4 fixed Amiga Paula channels.
  - sc68: dynamic count via sc68_scope_channels() (3 YM / 4 Paula); real static_destroy
    promoted to its new dedicated vtable slot.
  - v2m: per-synth-channel count via synthGetNumChannels(); lib patch mixes to mono so
    scope_width = 0 (no stereo).
- Each plugin's vendored API v2 header sync committed alongside the code (batch-1 precedent);
  the pre-existing build.yml CI tweak left uncommitted (unrelated to viz).
- Added 4 dlopen harness tests (core/tests/viz_{spu,uade,sc68,v2m}.rs) asserting caps,
  NULL pattern/vu getters, scope channel count/names/mono width, silence-before-enable,
  non-silence-after-enable.
- Reviewers: 2 (combined correctness+quality, test coverage).
- Rounds: 1, 0 blocking findings (1 non-blocking note: tests don't assert scroll_mode,
  matching the batch-1 template).
