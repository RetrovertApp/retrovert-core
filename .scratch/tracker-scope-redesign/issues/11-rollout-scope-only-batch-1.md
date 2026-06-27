# Rollout — scope-only batch 1

Status: ready-for-agent
Type: AFK
Est. context: ~110k

## Parent

Tracker/Scope redesign PRD (staged mechanical rollout).

## What to build

Migrate the first batch of **scope-only** plugins to the new viz API: advertise `caps = Scope` (add `Vu` only where cheaply available), implement `get_structure`, scope channel descriptors (with `scope_width`), explicit `set_scope_enabled`, and the new `get_scope_data`. No pattern cells. This is largely repetitive plumbing per plugin, but each still needs a from-source build and a harness check.

Batch (≈6, confirm each plugin's actual capability while migrating; if one turns out to have no real scope, give it `caps = 0` and move it to the no-viz batch):

- adplug, audio_stream, furnace, klystrack, mdxmini, pmdmini

## Acceptance criteria

- [ ] Each plugin advertises correct caps and implements the new scope getters (or `caps = 0` if it has no real visualization).
- [ ] Each builds from source and returns non-silent scope (where applicable) through the harness.
- [ ] No plugin in the batch references the old tracker/scope vtable slots afterward.

## Blocked by

- #03 (committed dlopen viz harness).
