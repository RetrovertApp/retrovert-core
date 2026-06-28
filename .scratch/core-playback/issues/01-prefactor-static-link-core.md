# Prefactor: static-link core into the console; drop the C-ABI core boundary

Status: done
Type: AFK
Est. context: ~75k

## What to build

Make `core` a normal Rust dependency of `retrovert-console` instead of a dlopen'd `.so`. Remove the C-ABI host boundary (the `#[no_mangle] core_*` entry points) and expose the core as plain Rust — `Core::new`, `Core::update`, `Core::load_url`, `Args`. The console links the core crate directly and calls these methods. Per ADR-0003.

Construction must not panic when zero plugins are present: `Core::new` returns `Result` and `main` logs + exits cleanly. Do **not** make the resampler optional inside `PlaybackInternal` (scope trap) — the panic today comes from the `.unwrap()`s on `Playback::new`/`Playlist::new` in `Core::new` turning the existing `bail!` into a panic.

Delete the orphaned modules `loader.rs`, `decoder.rs`, `resample.rs`, `plugin_settings.rs` (not in the module tree; `plugin_settings.rs` doesn't even compile). Move logging init (currently in `core-loader`) into the console or core; the console no longer uses `core-loader`.

`r2_front_display` depends on the removed `core_*` ABI — it is **accepted broken** and excluded from the default build (old frontend; console + Flowi UI are the targets).

Mechanical notes: `core/Cargo.toml` is `crate-type = ["dylib"]` only — add `rlib`; add a path dependency from the console to the core crate.

## Acceptance criteria

- [x] `retrovert-console` links the core crate directly (no dlopen, no `core_loader`); `cargo run -p retrovert-console` builds and runs.
- [x] No `#[no_mangle] core_*` exports remain; the host surface is Rust methods on `Core`.
- [x] Running with zero plugins logs a clean error and exits without panicking.
- [x] `loader.rs`/`decoder.rs`/`resample.rs`/`plugin_settings.rs` deleted; workspace builds.
- [x] `r2_front_display` no longer blocks the workspace build.

## Blocked by

- None - can start immediately.

## Comments

### 2026-06-28 — Implemented
- Commit: 76116c1 core: 01 static-link core into console
- Reviewers: 2 (combined correctness+quality, test coverage)
- Rounds: 1, 0 findings (1 non-blocking note: anyhow double-prints the error after the explicit `error!` log on `Core::new` failure — cosmetic, kept `Err` return for correct exit code)
- Note: new `retrovert-console` crate added inside the retrovert-core workspace (links `rv_core` directly); legacy `repos/retrovert-console` + `repos/core-loader` now unused, untouched. `r2_front_display` was never a workspace member, so it does not block the build.
