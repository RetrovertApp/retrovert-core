# Rollout — no-visualization cohort (caps = 0)

Status: ready-for-agent
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

- [ ] Each plugin advertises `caps = 0` (or is promoted to scope-only if it actually has a scope) and loads cleanly through the harness.
- [ ] Each builds from source and decodes audio as before (no regression).
- [ ] No plugin in the cohort references the old tracker/scope vtable slots afterward.

## Blocked by

- #03 (committed dlopen viz harness).
