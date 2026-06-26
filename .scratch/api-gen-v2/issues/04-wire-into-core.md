# Wire the generated module into plugin_types and fix core call sites

Status: done
Type: AFK
Est. context: ~85-90k

## What to build

Swap `plugin_types` from the stale hand-written `ffi_gen` over to the freshly generated `generated` module, and fix every `core` call site so the whole workspace builds against the v2 ABI. See **Task 4** of the plan: `docs/api_gen-v2-catchup-plan.md`.

Steps:
- Point `plugin_types/src/lib.rs` at `pub mod generated; pub use generated::*;` and delete `plugin_types/src/ffi_gen.rs`.
- Add hand-written version constants in `plugin_types/src/versions.rs` (`RV_PLAYBACK_PLUGIN_API_VERSION = 2`, output/resample = 1) — api_gen does not emit these — and re-export them.
- Fix the `core` call sites: Service pointer casts (`as *const _ as *const plugin_types::RVService`) at every `create`/`open`/`static_init`/`metadata`/`settings_updated`; `Option`-unwrap the generated fn-pointer fields (`.unwrap()` for required slots, `if let Some(..)` for the null-checked optional ones); bump the playback `api_version` check to `2` via the constant.

**Scope correction (reviewers): the plan's file list is incomplete.** The call sites and struct-literal breaks span MORE than `plugin_handler.rs` + `playback.rs`:
- Also fix `core/src/output.rs` (`create`, `start`, and the `PlaybackCallback` struct literal whose `callback` field becomes `Option<fn>` → wrap in `Some(...)`).
- Also fix `core/src/playlist.rs` (`create`/`open`/`destroy`) and `core/src/resample.rs` (`create`).
- The v2 ABI drops three fields still present in `ffi_gen.rs` and used in struct literals in `playback.rs`: `ReadInfo.virtual_channel_count`, `ReadData.virtual_channel_output`, `ReadData.virtual_channels_output_max_bytes_size`. Remove them from the literals.
- Grep to confirm completeness: `grep -rn 'static_init\|\.create)\|\.open)\|\.metadata)\|settings_updated\|virtual_channel' core/src`.

## Acceptance criteria

- [x] `plugin_types/src/lib.rs` uses `generated`; `ffi_gen.rs` deleted; `versions.rs` added and re-exported.
- [x] `cargo build -p plugin_types` compiles (incl. the generated `size_of`/`offset_of` static-asserts — the layout guard).
- [x] All Service-pointer casts and `Option`-unwrapped fn-pointer calls applied across `plugin_handler.rs`, `playback.rs`, `output.rs`, `playlist.rs`, `resample.rs`.
- [x] No `virtual_channel*` fields remain in any struct literal; no literal `1` remains for the playback `api_version` check.
- [x] `cargo build` (whole workspace) is green.
- [x] Committed on a `retrovert-core` branch (e.g. `api-gen-v2`).

## Blocked by

- #3 (codegen bin) — produces `plugin_types/src/generated/`

## Comments

### 2026-06-26 — Implemented
- Commit: branch `api-gen-v2`, "plugin_types: wire generated v2 ABI into core"
- Reviewers: 2 (combined correctness+quality, test-coverage) — both APPROVE, round 1
- Rounds: 1, 1 non-blocking note (moved a `ponytail:` debt comment out of rustdoc)
- Beyond the issue's stated steps (all required for a green build): added `ext.rs` shim for
  derives/`Send`/`Sync`/`get_name`/`get_version` api_gen doesn't emit; `lib.rs` flattens the
  generated submodules (their code uses `crate::` paths) with one canonical `RVService`;
  casts use `as *const _` (the three `RVService` types are distinct per submodule); `probe_can_play`
  data arg cast to `*mut`; `output_callback` made a safe `extern "C" fn`. No core-side `api_version`
  check exists, so that criterion is vacuous. 20 `unnecessary unsafe` warnings remain in `rv_core`
  (generated fn-pointers are typed safe `extern "C" fn`); kept the `unsafe` — fix belongs in api_gen.
