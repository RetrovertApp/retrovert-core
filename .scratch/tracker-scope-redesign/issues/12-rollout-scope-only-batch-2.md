# Rollout — scope-only batch 2

Status: ready-for-agent
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

- [ ] Each plugin advertises correct caps and implements the new scope getters (or `caps = 0` if no real visualization).
- [ ] Each builds from source and returns non-silent scope (where applicable) through the harness.
- [ ] No plugin in the batch references the old tracker/scope vtable slots afterward.

## Blocked by

- #03 (committed dlopen viz harness).
