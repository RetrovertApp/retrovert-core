# 0001 — Generated ABI bindings are committed; the generator lives with the flowi-dependent app

## Status

accepted (2026-06-26)

## Context

Retrovert is adopting Flowi's `api_gen` (an IDL → C + Rust code generator) as the single source of
truth for the Playback Plugin ABI, replacing hand-carried FFI. `api_gen` lives inside the Flowi repo
and depends on the Flowi Rust toolchain.

The consumers of the generated code are:

- `retrovert_api` — owns the `.def` and the generated **C headers** (the shared contract, also used by
  RePlay; see CONTEXT.md).
- `retrovert-core` — owns the generated **Rust FFI** (`plugin_types/ffi_gen.rs`) and loads compiled
  plugins against it.

Neither of these repos otherwise depends on Flowi, and we want to keep it that way: `retrovert_api` is
a tiny headers-and-`.def` repo, and `retrovert-core` is the player core. Pulling Flowi into either —
just so a build-time codegen step can run — would couple the ABI/core layer to a large, actively-
migrating UI framework. We also explicitly decided **not** to split `api_gen` out of Flowi into a
standalone tool right now.

## Decision

**Generated bindings are committed artifacts. The only node that depends on Flowi is the thing that
runs the generator — the `retrovert` app — and the ABI/core repos stay flowi-free pure consumers.**

- `retrovert_api/*.h` and `retrovert-core/plugin_types/ffi_gen.rs` are **committed** generated files.
  The C and Rust *builds* see ordinary committed source — no codegen at build time, no Flowi toolchain
  required to build them.
- The **codegen bin** lives in the `retrovert` app repo (which already path-deps Flowi:
  `flowi = { path = "../../flowi/rust/flowi" }`). It path-deps `api_gen` and writes into sibling
  `retrovert_api` / `retrovert-core` by relative path.
- Regeneration is a deliberate maintainer action; the "regenerate + `git diff --exit-code`" gate (when
  CI exists) lives with the app, where Flowi is present.

This mirrors Flowi's own build model (its ADR-0030: output committed + CI-gated; the C build sees
ordinary headers).

## Considered options

- **In-repo generator + CI gate in each consumer (Flowi's own pattern).** Rejected: Flowi can host its
  generator in-repo because it *is* the Flowi repo. Replicating that for `retrovert_api` / `retrovert-core`
  means each grows a Flowi build dependency for a dev-time-only step — exactly the coupling we're avoiding.

- **Split `api_gen` into a standalone shared crate** both Flowi and Retrovert depend on. Cleanest
  long-term, but explicitly deferred — too much upfront work to unblock the v2 drift now.

- **Pin Flowi as a git submodule in `retrovert-core`.** More reproducible, but heavier than the repo's
  existing flowi-by-path convention, and still parks a Flowi dependency in the core repo. Revisit when
  the app and Flowi stop being side-by-side checkouts.

## Consequences

- `retrovert_api` and `retrovert-core` can be built by anyone with no Flowi checkout.
- Regenerating bindings requires a Flowi checkout and is done from the `retrovert` app — a deliberate,
  reviewable act (and the moment to re-run the byte-compat checks).
- RePlay consumes the same `.def` with its own output config from its own repo, symmetrically — it
  doesn't reach into Retrovert for the contract.
- The path-dep assumes Flowi and `retrovert` are side-by-side checkouts; when that stops holding, switch
  to a pinned submodule (tracked in the design doc's follow-ups).
