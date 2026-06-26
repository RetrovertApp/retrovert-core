# api_gen setup — design (milestone 1: clear the v1→v2 drift)

Date: 2026-06-26
Status: approved, pre-implementation

## Goal

Adopt Flowi's `api_gen` (`~/code/projects/flowi/rust/tools/api_gen`) as the single codegen
tool for Retrovert's plugin ABI, and use it to regenerate `plugin_types/src/ffi_gen.rs` so the
Rust core matches the current **v2** playback plugin ABI. This clears Blocker #1: the Rust core's
generated FFI is stuck at v1 and cannot load the 2026 v2 plugins.

This milestone is **catch-up only** — reproduce the existing v2 ABI byte-for-byte, no API changes.
The tracker/scope API redesign is a separate, later ABI bump (don't conflate a codegen bug with an
API-design bug).

## Background (verified state)

- `api_gen` is general-purpose: IDL (`.def`, Pest grammar) → C headers + raw Rust FFI (`flowi_sys`)
  + optional ergonomic Rust façade (`flowi`). Naming is pluggable (`naming.rs`: `type_prefix`,
  `func_prefix` take arbitrary strings, so `RV`/`rv` is just a config value). Input/output **paths are
  hardcoded in `main.rs`**; there is no config file and no lib API (binary only). (Note: flowi's
  reserved `Rp`/"Replay plugin" profile is about exposing *flowi's own UI API* to RePlay as a host —
  unrelated to retrovert's playback codegen; see CONTEXT.md.)
- `api_gen` generates **Direction 1** only: vtable structs + `extern "C" { ... }` declarations that
  Rust *calls into* (e.g. `vfs_plugin.def` → `FlVfsPlugin` struct). It does **not** generate
  **Direction 2**: the host-implements-a-service-and-exposes-a-C-vtable glue (wrapper fn bodies that
  cast `*mut c_void` and a vtable builder). Adding Direction 2 ≈ 400–700 lines of new `rust_gen.rs`.
- The v2 ABI has **no current `.def`** — the v2 visualization surface was hand-written straight into
  `retrovert_api/include/retrovert/playback.h`; the old `.def` still says v1. The old retrovert
  `api_generator` cannot run (its `apigen` parser crate is missing). So the `.def` is authored fresh.
- Where the drift actually lives:
  | Crate | FFI direction | Version | On critical path? |
  |---|---|---|---|
  | `plugin_types` (playback **v2**, output, resample vtables) | 1 — host calls plugin | playback = **v2 drift** | yes — this *is* the blocker |
  | `services` (io/log/metadata/settings) | 2 — host implements, plugin calls back | v1, not drifted | no — not broken |

  The v1→v2 drift is entirely in the **playback plugin vtable** (Direction 1). Services are v1 on both
  sides and untouched. **So milestone 1 needs only Direction 1, which api_gen already does.**

## api_gen feasibility for the RV ABI (verified)

api_gen can reproduce the RV v2 ABI byte-for-byte **except fixed-size arrays in struct fields**
(`layout.rs` deliberately bails on them and panics). What works as-is: `RV`/`rv` naming, plain
`const char*` (via `*const char`, not flowi's `String`/`FlString`), the vtable-of-fn-pointers,
`extern opaque` type references, and optional/nullable fn-pointer slots. Not generated (but
hand-written today anyway, so no regression): the per-API version `#define` and the
`rv_playback_plugin()` entry symbol.

**The fixed arrays live only in the visualization structs** — `RVTrackerInfo`
(`char song_name[64]`, `RVChannelInfo channels[8]`, `char sample_names[32][24]`…) and `RVChannelInfo`
(`char name[16]`). Milestone 1 sidesteps them: load-and-play never reads those structs, and the
vtable references them **by pointer**, so declaring `RVTrackerInfo` / `RVChannelInfo` / `RVPatternCell`
as `extern opaque` keeps the vtable byte-identical while their bodies stay hand-written (as they are
now). The tracker/scope redesign reshapes these structs *away from* fixed arrays anyway, so the array
feature is deferred and may never be needed. **Net: the only api_gen change milestone 1 requires is the
lib+bin Config refactor.**

## Design

### 1. Refactor api_gen → `lib + thin bin` (change in Flowi)

Extract the hardcoded paths + naming from `main.rs` into:

```rust
pub struct Config {
    pub naming: Naming,           // type_prefix, func_prefix
    pub api_dir: PathBuf,         // input: <dir>/*.def
    pub c_out: PathBuf,           // C headers
    pub rust_sys_out: PathBuf,    // raw Rust FFI
    pub rust_facade_out: Option<PathBuf>, // ergonomic façade; None = skip
}
pub fn generate(cfg: &Config) -> io::Result<()>;
```

Flowi's existing `bin` calls `generate(&flowi_config)` — **no behavior change**; flowi's CI gate
(`gen_bindings.sh && git diff --exit-code`) proves the output stays byte-identical after the refactor.
~100–150 lines of plumbing. This is the only change to Flowi.

### 2. The `.def` lives in `retrovert_api` (shared contract); each consumer has its own output config

The Plugin ABI is **shared with RePlay** (a separate frontend that loads the same Playback Plugins),
so the `.def` is not retrovert's private asset — it's the source of the shared C contract.
`retrovert_api` is already that master: it owns the hand-written headers today, and
`update-api-headers.sh` (`cp -r`) fans them into every plugin's vendored
`include/retrovert/playback.h`, which both frontends consume via the `playback_plugins` submodule.

```
retrovert_api  (master: .def + generated headers)
      │ cp -r (update-api-headers.sh, unchanged)
      ▼
playback_plugins/plugins/*/include/retrovert/playback.h   ← vendored copies
      ▲ submodule
      ├── replay_frontend     (RePlay: own bindings config, later)
      └── retrovert-core      (Rust FFI: plugin_types/ffi_gen.rs)
```

So:
- **`.def` lives in `retrovert_api`** (e.g. `retrovert_api/api/*.def`). One shared source of truth.
- api_gen generates the **C headers into `retrovert_api`**; the existing `cp -r` propagation is
  untouched.
- The codegen bin lives in the **`retrovert` app repo** (see §2a), runs api_gen with the
  **Rust-output** config:
  - naming = `RV` / `rv` (uppercase `RV` type prefix — fixed by the shipped plugins)
  - `api_dir` = `retrovert_api/api/`
  - `c_out` = a scratch dir for milestone 1 (diff against the committed `retrovert_api/.../playback.h`;
    do **not** overwrite the hand-written master until the diff is clean — see Proof). Repoint at the
    real `retrovert_api` headers once verified.
  - `rust_sys_out` = `retrovert-core/plugin_types/src/`
  - `rust_facade_out` = **None** (core works on raw vtables; the façade is UI sugar)
- RePlay later adds its own output config against the same `.def` — out of scope for this milestone.

No second copy of the generator exists → no re-drift.

### 2a. The generator lives with the flowi-dependent app; consumers stay flowi-free

Generated artifacts are **committed**, and `retrovert_api` + `retrovert-core` stay **flowi-free pure
consumers** — they only ever hold committed generated files, never build or link flowi. The flowi
dependency enters at exactly one node: the thing that runs the generator. That node is the **`retrovert`
app repo**, which already path-deps flowi (`flowi = { path = "../../flowi/rust/flowi" }`). So:

- The codegen bin is a tool in the `retrovert` repo, path-dep'ing `../../flowi/rust/tools/api_gen`
  (the refactored lib) and targeting sibling `../retrovert_api` and `../retrovert-core` by relative
  path. Matches the repo's existing flowi-by-path convention (no submodule).
- This mirrors flowi's own model (ADR-0030): generated output is committed + gated; the C/Rust *builds*
  see ordinary committed files, no codegen at build time.
- Why this avoids splitting api_gen out of flowi (explicitly deferred): the generator stays inside
  flowi; only the flowi-dependent app invokes it.

This decision is recorded in `docs/adr/0001-generated-abi-committed-generator-lives-with-app.md`.

### 3. `.def` scope — Direction-1 surface, array-free

Author `.def` files reproducing the current v2 ABI:

- **Plugin vtables:** `PlaybackPlugin` (v2 — the **full** vtable incl. the viz fn-pointer slots
  `get_tracker_info` / `get_pattern_cell` / `get_pattern_num_rows` / `get_scope_data` /
  `get_scope_channel_names` / `static_destroy`, since those slots are what clears the drift),
  `OutputPlugin`, `ResamplePlugin`.
- **Data structs/enums (array-free ones):** `AudioFormat`, `ReadInfo`, `ReadData`, `ProbeResult`,
  `ReadStatus`, `ConvertConfig`, `OutputTargets`, `PlaybackCallback`, `RVPatternCell` (all-scalar),
  etc. — the array-free subset of what `plugin_types/ffi_gen.rs` + the v2 headers contain.
- **Viz structs with fixed arrays** — `RVTrackerInfo`, `RVChannelInfo` — declared **`extern opaque`**.
  The vtable references them by pointer, so this keeps the vtable byte-identical; their bodies stay
  hand-written (status quo) until the redesign reshapes them. (See feasibility section.)
- **`ServiceFFI` / `RVService`** referenced as **`extern opaque`**, pointing at the existing
  hand-written `services` crate type. Services are NOT regenerated here.
- **Hand-written, unchanged:** the per-API version `#define` and the `rv_playback_plugin()` entry
  symbol (api_gen doesn't emit these; they're hand-written today regardless).

Target output: a regenerated `plugin_types/src/ffi_gen.rs` byte-compatible with the v2 C ABI the
existing plugins were built against. api_gen's size/offset static-asserts pin the layout.

### 4. Out of scope (deferred, tracked)

- **Fixed-size struct-array support in api_gen** (~250 LOC: `layout.rs` + `c_gen.rs` + `rust_gen.rs`).
  Deferred to the tracker/scope redesign, which reshapes the only structs that need it (and away from
  fixed arrays — so this may never be built).
- **Direction-2 codegen** (the `services` crate glue). Services aren't drifted; build the feature only
  when services are next touched (e.g. a service redesign), to kill the last hand-carried FFI file.
- **The tracker/scope API redesign** — separate ABI bump after this known-good checkpoint lands.
- Output/resample C headers (the old `.def` defined them but no headers were ever generated) — include
  them in the `.def` since they're Direction 1 and cheap, but they're not what unblocks playback.

## Proof of done

1. **Static check:** the generated C surface (vtable, array-free structs, and the `extern opaque`
   forward-decls) matches the corresponding parts of the current hand-written `playback.h`; the
   hand-written viz struct bodies (`RVTrackerInfo`/`RVChannelInfo`) remain in a hand-written header the
   generated one includes. So this is a diff over the *generated* surface, not the whole file. The
   real layout guard is api_gen's size/offset static-asserts on the Rust side — they must compile.
2. **Runtime check (the decisive one):** an existing compiled v2 plugin loads via `libloading` and
   plays a file end-to-end through `retrovert-console`. This proves the regenerated vtable is
   byte-compatible with what the shipped plugins expose.

When both pass: Blocker #1 is cleared, and the new tool is the source of truth for `plugin_types`.

## Open follow-ups

- When `retrovert` (the app) and flowi stop being side-by-side checkouts, revisit path-dep vs a
  pinned flowi submodule (matching `replay_frontend`'s `external/flowi`). Path-dep for now.
- `clone-base.py` still references old underscore plugins — fix when getting a clean end-to-end play
  (separate from codegen, but on the path to the runtime check).
