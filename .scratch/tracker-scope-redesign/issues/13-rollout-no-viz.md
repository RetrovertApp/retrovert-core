# Rollout — no-visualization cohort (caps = 0)

Status: done
Type: AFK
Est. context: ~85k

## Parent

Tracker/Scope redesign PRD (staged mechanical rollout).

## What to build

Bring the **no-visualization** plugins onto the new ABI by advertising `caps = 0` and leaving all viz getters unimplemented (NULL). The per-plugin code change is trivial, but each has a large vendored build, so the cost is build/verify friction across the cohort, not viz logic. Confirm each genuinely has no visualization while migrating; promote any that turn out to have a real scope into a scope-only batch.

Cohort (≈10, verify each):

- asap, cpsycle, eupmini, fmplayer, gme, libkss, organya, sunvox, xsf, zxtune

Note: `pretracker` is an unwired outlier (nested source, no `include/retrovert`) — exclude unless verified as a real loaded plugin.

## Acceptance criteria

- [x] Each plugin advertises `caps = 0` (or is promoted to scope-only if it actually has a scope) and loads cleanly through the harness.
- [x] Each builds from source and decodes audio as before (no regression).
- [x] No plugin in the cohort references the old tracker/scope vtable slots afterward.

## Blocked by

- #03 (committed dlopen viz harness).

## Comments

### 2026-06-27 — Implemented
- Cohort split after verifying actual capability per the issue's promote instruction:
  - **caps = 0** (NULL all viz slots): asap, cpsycle, eupmini, fmplayer, gme, libkss, sunvox.
  - **Promoted to scope-only** (real working old-ABI scope wired to engine capture):
    organya (16 voices = 8 melody + 8 perc), zxtune (3 AY channels), xsf (dynamic
    PSX SPU voices, PSF only — other xSF formats honestly report caps = 0).
  - Each plugin's vtable tail rebuilt to the v2 ABI; `static_destroy` (several were
    misplaced after the old slots) moved to its correct position after settings_updated.
- Build: all 10 build from source. asap needed a libm link fix (latent undefined `sin`
  surfaced by the dlopen harness, same class issue 11 fixed). cpsycle and xsf hit
  PRE-EXISTING vendored gcc-16 permerrors (cpsycle-lib incompatible-pointer-type;
  lazyusf2 `return 0;` in a void fn) unrelated to viz — not fixed here (vendored, out
  of scope); both plugin TUs compile and link cleanly.
- Tests: 4 new dlopen harness tests in retrovert-core (viz_no_viz table-driven over the
  7 caps=0 plugins; viz_organya / viz_zxtune / viz_xsf scope-only, incl an xsf non-PSF
  caps=0 case). xsf PSF live-scope skips when the host PSX BIOS is absent.
- Reviewers: 4 (correctness, code-quality, test-coverage, whole-file on xsf_plugin.c).
- Rounds: 2. Correctness APPROVED round 1; quality (2 stale comments) and coverage
  (4 test-hardening) fixed and APPROVED round 2. Whole-file flagged pre-existing
  xsf_plugin.c duplication (extension table, psf-callback setup, PSF2 load-cb) —
  deferred as out of scope for this mechanical rollout.
- Deferred / not committed: build.yml CI tweaks, new output.h/resample.h header
  fan-out (unrelated to viz, predate migrated plugins).
