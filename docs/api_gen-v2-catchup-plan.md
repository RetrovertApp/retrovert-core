# api_gen v2 Catch-Up — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Regenerate Retrovert's Rust plugin FFI (`plugin_types`) from `.def` files via Flowi's `api_gen`, matching the current **v2** C plugin ABI, so the core can load and play today's v2 plugins.

**Architecture:** Flowi's `api_gen` is refactored into `lib + thin bin` driven by a `Config` (naming + paths). A new dev-only codegen bin in the `retrovert` app runs it with an `RV`/`rv` config against `.def` files authored in `retrovert_api`, emitting C headers (scratch, for diffing) and Rust FFI into `retrovert-core/plugin_types`. The viz structs with fixed arrays stay `extern opaque`; the array feature is deferred. Generated artifacts are committed; `retrovert_api`/`retrovert-core` stay flowi-free (see ADR-0001).

**Tech Stack:** Rust (edition 2021), Flowi `api_gen` (Pest-based IDL→C+Rust), `libloading`, Cargo.

## Start here (bootstrap for a clean context)

You may be running this with no prior conversation. Everything you need is committed or in the repos.

- **Read first (the "why"), all in `~/code/projects/retrovert/repos/retrovert-core`:**
  `CONTEXT.md` (glossary: Retrovert vs RePlay, Plugin ABI, Service, Flowi), `docs/api_gen-setup-design.md`
  (the design + rationale), `docs/adr/0001-generated-abi-committed-generator-lives-with-app.md`.
- **Repos & branches involved** (all under `~/code/projects/`):
  | Repo | Path | Branch to use | Role |
  |---|---|---|---|
  | flowi | `~/code/projects/flowi` | `api-gen-config` (create) | owns `api_gen`; Task 1 refactors it |
  | retrovert_api | `~/code/projects/retrovert/repos/retrovert_api` | `api-gen-defs` (create) | `.def` source + generated C headers |
  | retrovert (app) | `~/code/projects/retrovert/repos/retrovert` | `abi-gen-tool` (create) | hosts the codegen bin; path-deps flowi |
  | retrovert-core | `~/code/projects/retrovert/repos/retrovert-core` | `api-gen-v2` (create) | generated Rust FFI consumer |
- **Order matters:** do Task 1 first and keep flowi **on its `api-gen-config` branch** for the rest of
  the run — Task 3's `path = ".../flowi/rust/tools/api_gen"` dep compiles against whatever is checked out.
- **Resolving an `api_gen` parse error** (Task 3 Step 4): consult the cheat-sheet in this plan first;
  for anything it doesn't cover, read flowi's grammar `~/code/projects/flowi/rust/tools/api_gen/src/api.pest`
  and the worked example `~/code/projects/flowi/rust/tools/api_gen/api/vfs_plugin.def` (a vtable + `extern
  opaque` + enums — the closest analog). Primitives/pointers are fixed-width; there is no IDL `int` (use `i32`).
- **Verification inputs you must locate** (Task 5, not pinned here): a compiled v2 playback plugin (build
  output under `playback_plugins`), a sample module file to play, and `retrovert-console`'s CLI — check
  its `--help`/arg parsing in `retrovert-core/retrovert-console` (or the binary's `show_args`).
- **Run the codegen** from the retrovert app repo root: `cargo run --manifest-path tools/abi_gen/Cargo.toml`.

## Global Constraints

- **Byte-compat is the contract.** The regenerated `plugin_types` must be layout-identical to the v2 C ABI in `retrovert_api/include/retrovert/*.h`. The decisive test is loading a real compiled v2 plugin; the C-surface diff and the Rust `size_of`/`offset_of` asserts are the static guards.
- **Type prefix `RV`, func prefix `rv`** — fixed by the 29 shipped plugins (`RVPlaybackPlugin`, `rv_playback_plugin`). Non-negotiable.
- **C module = `retrovert`** — generated headers must land at `<c_root>/retrovert/<name>.h`.
- **No fixed-size struct arrays** in any authored `.def` (api_gen panics on them). The only array-bearing structs (`RVTrackerInfo`, `RVChannelInfo`) are `extern opaque`.
- **`int`/`int32_t`:** author C-`int` parameters as IDL `i32` (emits `int32_t`, ABI-identical on LP64; the v1 Rust already uses `i32`).
- **Flowi changes land on a Flowi branch first**, then the `retrovert` app pins/points at that Flowi. Do not edit Flowi `main` in place beyond the reviewed refactor.
- **Commit generated output**; the regen + `git diff` check lives with the app (where Flowi is present), not in the consumer repos.

## Reference: `.def` authoring cheat-sheet

```
#[module(retrovert)]                         // C headers → <c_root>/retrovert/<name>.h
extern opaque RVService                      // reference a hand-written type by pointer only
enum ProbeResult { Supported = 0, ... }      // C: RVProbeResult_Supported ; Rust: ProbeResult::Supported
#[attributes(Copy)]
struct ReadInfo { format: AudioFormat, frame_count: u32, status: ReadStatus }
struct Plugin {                              // a vtable = scalar/ptr fields + fn-pointer fields
    api_version: u64,
    name: *const char,                       // C: const char* ; Rust: *const core::ffi::c_char
    create(services: *const RVService) -> *void,   // fn-pointer field; Rust wraps in Option<>
    read_data(user_data: *void, dest: ReadData) -> ReadInfo,
}
// pointer tokens: *void  *const char  *u8  *const u8  *<Type>  *const <Type>
// primitives: void char bool i8 u8 i16 u16 i32 u32 i64 u64 f32 f64 usize isize
// Rust types are UNPREFIXED; extern-opaque keeps its literal name; version #defines are hand-written.
```

---

### Task 1: Refactor Flowi `api_gen` into `lib + thin bin` with a `Config`

**Repo:** `~/code/projects/flowi` (work on a branch, e.g. `api-gen-config`).

**Files:**
- Create: `rust/tools/api_gen/src/lib.rs`
- Modify: `rust/tools/api_gen/src/main.rs` (replace body with a Flowi `Config` + `generate()` call)

**Interfaces:**
- Produces: `pub struct Config { pub naming: Naming, pub api_dir: PathBuf, pub rust_dir: PathBuf, pub c_include_root: PathBuf, pub facade_dir: Option<PathBuf> }` and `pub fn generate(cfg: &Config)`. Consumed by Task 3.

- [ ] **Step 1: Create `lib.rs` exposing `Config` + `generate()`**

Move the current `main()` body (api_gen/src/main.rs:29-152) into a library function. Create `rust/tools/api_gen/src/lib.rs`:

```rust
//! api_gen as a library: one `generate(&Config)` entry, so multiple projects
//! (Flowi, Retrovert) drive it with their own naming + paths. See ADR-0030/0032.

mod api_parser;
mod c_gen;
mod layout;
pub mod naming;
mod rust_gen;

#[macro_use]
extern crate pest_derive;

use crate::api_parser::{ApiDef, ApiParser};
use crate::c_gen::Cgen;
use crate::naming::Naming;
use crate::rust_gen::RustGen;
use std::path::PathBuf;

/// One project's codegen target: naming profile + input/output paths.
pub struct Config {
    pub naming: Naming,
    /// Input defs: `<api_dir>/*.def`.
    pub api_dir: PathBuf,
    /// Raw Rust FFI output dir (Flowi: `rust/flowi_sys/src/generated`).
    pub rust_dir: PathBuf,
    /// C public-header root; per-module subdir created on demand.
    pub c_include_root: PathBuf,
    /// Ergonomic Rust façade output dir. `None` skips façade generation.
    pub facade_dir: Option<PathBuf>,
}

pub fn generate(cfg: &Config) {
    let api_dir = &cfg.api_dir;
    let rust_dir = cfg.rust_dir.to_str().unwrap();
    let c_include_root = cfg.c_include_root.to_str().unwrap();

    std::fs::create_dir_all(c_include_root)
        .unwrap_or_else(|e| panic!("api_gen: creating {}: {}", c_include_root, e));
    std::fs::create_dir_all(rust_dir)
        .unwrap_or_else(|e| panic!("api_gen: creating {}: {}", rust_dir, e));
    if let Some(facade) = &cfg.facade_dir {
        std::fs::create_dir_all(facade)
            .unwrap_or_else(|e| panic!("api_gen: creating {}: {}", facade.display(), e));
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(api_dir)
        .unwrap_or_else(|e| panic!("api_gen: cannot read {}: {}", api_dir.display(), e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "def").unwrap_or(false))
        .collect();
    files.sort();

    let mut api_defs: Vec<ApiDef> = Vec::with_capacity(files.len());
    for path in &files {
        let mut api_def = ApiDef::default();
        ApiParser::parse_file(path, &mut api_def);
        api_defs.push(api_def);
    }

    ApiParser::second_pass(&mut api_defs, &cfg.naming);
    api_defs.sort_by(|a, b| a.filename.cmp(&b.filename));

    let registry = layout::TypeRegistry::build(&api_defs);

    let mut type_loc: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    for api_def in &api_defs {
        let loc = (api_def.module.clone(), api_def.base_filename.clone());
        for sdef in &api_def.structs {
            type_loc.insert(sdef.name.clone(), loc.clone());
        }
        for edef in &api_def.enums {
            type_loc.insert(edef.name.clone(), loc.clone());
        }
        for cb in &api_def.callbacks {
            type_loc.insert(cb.name.clone(), loc.clone());
        }
    }

    RustGen::generate_mod_file(rust_dir, &api_defs)
        .unwrap_or_else(|e| panic!("api_gen: writing mod.rs: {}", e));

    let mut facade_modules: Vec<String> = Vec::new();
    for api_def in &api_defs {
        RustGen::generate(rust_dir, api_def, &registry)
            .unwrap_or_else(|e| panic!("api_gen: writing rust for {}: {}", api_def.base_filename, e));
        if let Some(facade) = &cfg.facade_dir {
            if RustGen::generate_facade(facade.to_str().unwrap(), api_def)
                .unwrap_or_else(|e| panic!("api_gen: façade {}: {}", api_def.base_filename, e))
            {
                facade_modules.push(api_def.base_filename.clone());
            }
        }
        if api_def.has_real_types() || api_def.has_apis() || api_def.has_callbacks() {
            Cgen::generate_header(c_include_root, api_def, &type_loc, &cfg.naming)
                .unwrap_or_else(|e| panic!("api_gen: C header {}: {}", api_def.base_filename, e));
        }
    }

    if let Some(facade) = &cfg.facade_dir {
        RustGen::generate_facade_mod_file(facade.to_str().unwrap(), &facade_modules)
            .unwrap_or_else(|e| panic!("api_gen: façade mod.rs: {}", e));
    }

    println!("api_gen: generated {} module(s) -> {}", api_defs.len(), rust_dir);
}
```

Note: this hoists `naming` to `pub mod`, threads `cfg.naming` into `second_pass` and `generate_header` (replacing the two `naming::FL` hardcodes at main.rs:79,130), and gates façade generation on `cfg.facade_dir`.

- [ ] **Step 2: Replace `main.rs` with the Flowi `Config` (preserve current behavior)**

Rewrite `rust/tools/api_gen/src/main.rs` to compute Flowi's paths exactly as before and call the lib:

```rust
//! api_gen Flowi entry point — builds the Flowi Config and delegates to the lib.
//! Paths resolve relative to this crate's manifest, so cwd doesn't matter.

use api_gen::{generate, naming, Config};
use std::path::Path;

fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest
        .parent().and_then(Path::parent).and_then(Path::parent)
        .expect("api_gen must live at <repo>/rust/tools/api_gen");
    generate(&Config {
        naming: naming::FL,
        api_dir: manifest.join("api"),
        rust_dir: repo.join("rust/flowi_sys/src/generated"),
        c_include_root: repo.join("flowi/include/flowi"),
        facade_dir: Some(repo.join("rust/flowi/src/generated")),
    });
}
```

`naming::FL` is a `const`; passing it by value into `Config { naming: ... }` is fine (it's `Naming`, owned). If the borrow checker complains because `FL` is a `const Naming` with `Cow::Borrowed`, it still moves a fresh value — no change needed.

- [ ] **Step 3: Add the `[lib]`/`[[bin]]` split to `Cargo.toml`**

In `rust/tools/api_gen/Cargo.toml`, ensure both a lib and bin target exist (Cargo infers `src/lib.rs` + `src/main.rs` automatically for a package; only add explicit `[lib]`/`[[bin]]` if the package name differs). Confirm the package is named `api_gen`:

Run: `grep -A2 '\[package\]' ~/code/projects/flowi/rust/tools/api_gen/Cargo.toml`
Expected: `name = "api_gen"`.

- [ ] **Step 4: Regenerate and prove byte-identical output (the regression test)**

Run Flowi's existing gate — this is the test that the refactor changed nothing:

```bash
cd ~/code/projects/flowi
scripts/gen_bindings.sh
git diff --exit-code -- rust/flowi_sys/src/generated rust/flowi/src/generated flowi/include/flowi
```
Expected: `gen_bindings.sh` runs clean; `git diff --exit-code` returns **0** (no changes). If non-empty, the refactor altered output — fix until the diff is empty.

- [ ] **Step 5: Run api_gen's unit tests**

Run: `cd ~/code/projects/flowi && cargo test -p api_gen`
Expected: PASS (naming tests etc. unaffected).

- [ ] **Step 6: Commit (on the Flowi branch)**

```bash
cd ~/code/projects/flowi
git add rust/tools/api_gen/src/lib.rs rust/tools/api_gen/src/main.rs rust/tools/api_gen/Cargo.toml
git commit -m "refactor(api_gen): extract lib + Config so other projects can drive it"
```

---

### Task 2: Author the Retrovert `.def` files in `retrovert_api`

**Repo:** `~/code/projects/retrovert/repos/retrovert_api`.

**Files:**
- Create: `api/audio_format.def`, `api/playback.def`, `api/output.def`, `api/resample.def`

**Interfaces:**
- Produces: the IDL that Task 3 generates from. Type names (unprefixed) must match what `core` expects: `AudioFormat`, `ReadInfo`, `ReadData`, `ProbeResult`, `ReadStatus`, `SettingsUpdate`, `PatternCell`, `PlaybackPlugin`, `OutputPlugin`, `ResamplePlugin`.

- [ ] **Step 1: Write `api/audio_format.def`**

```
#[module(retrovert)]

enum AudioStreamFormat {
    U8 = 1,
    S16 = 2,
    S24 = 3,
    S32 = 4,
    F32 = 5,
}

#[attributes(Copy)]
struct AudioFormat {
    audio_format: AudioStreamFormat,
    channel_count: u32,
    sample_rate: u32,
}
```

(Matches `audio_format.h`: `RVAudioStreamFormat` + `RVAudioFormat`.)

- [ ] **Step 2: Write `api/playback.def`**

```
#[module(retrovert)]

extern opaque RVService
extern opaque RVTrackerInfo
extern opaque RVChannelInfo

enum ProbeResult {
    Supported = 0,
    Unsupported = 1,
    Unsure = 2,
}

enum ReadStatus {
    DecodingRequest = 0,
    Ok = 1,
    Finished = 2,
    Error = 3,
}

enum SettingsUpdate {
    Default = 0,
    RequireRestart = 1,
}

#[attributes(Copy)]
struct ReadInfo {
    format: AudioFormat,
    frame_count: u32,
    status: ReadStatus,
}

#[attributes(Copy)]
struct ReadData {
    channels_output: *void,
    channels_output_max_bytes_size: u32,
    info: ReadInfo,
}

#[attributes(Copy)]
struct PatternCell {
    note: u8,
    instrument: u8,
    volume: u8,
    effect: u8,
    effect_param: u8,
    dest_channel: u8,
}

struct PlaybackPlugin {
    api_version: u64,
    name: *const char,
    version: *const char,
    library_version: *const char,
    probe_can_play(data: *u8, data_size: u64, filename: *const char, total_size: u64) -> ProbeResult,
    supported_extensions() -> *const char,
    create(services: *const RVService) -> *void,
    destroy(user_data: *void) -> i32,
    event(user_data: *void, data: *u8, data_size: u64),
    open(user_data: *void, url: *const char, subsong: u32, services: *const RVService) -> i32,
    close(user_data: *void),
    read_data(user_data: *void, dest: ReadData) -> ReadInfo,
    seek(user_data: *void, ms: i64) -> i64,
    metadata(url: *const char, services: *const RVService) -> i32,
    static_init(services: *const RVService),
    settings_updated(user_data: *void, services: *const RVService) -> SettingsUpdate,
    get_tracker_info(user_data: *void, info: *RVTrackerInfo) -> i32,
    get_pattern_cell(user_data: *void, pattern: i32, row: i32, channel: i32, cell: *PatternCell) -> i32,
    get_pattern_num_rows(user_data: *void, pattern: i32) -> i32,
    get_scope_data(user_data: *void, channel: i32, buffer: *f32, num_samples: u32) -> u32,
    static_destroy(),
    get_scope_channel_names(user_data: *void, names: *const *const char, max_channels: u32) -> u32,
}
```

Notes: `RVService`/`RVTrackerInfo`/`RVChannelInfo` are `extern opaque` (referenced by pointer; bodies stay hand-written). `SettingsUpdate` is generated (returned by value). `RVChannelInfo` is opaque only because `RVTrackerInfo` embeds it — it's never named here directly, so its `extern opaque` line can be dropped if api_gen rejects an unused declaration (resolve in Step 5).

- [ ] **Step 3: Write `api/output.def`**

```
#[module(retrovert)]

extern opaque RVService

#[attributes(Copy)]
struct WriteInfo {
    sample_rate: u32,
    sample_count: u16,
    channel_count: u8,
    output_format: u8,
}

#[attributes(Copy)]
struct PlaybackCallback {
    user_data: *void,
    callback(user_data: *void, data: *void, format: AudioFormat, frames: u32) -> u32,
}

#[attributes(Copy)]
struct OutputTargets {
    names: *const *const char,
    names_size: u64,
}

struct OutputPlugin {
    api_version: u64,
    name: *const char,
    version: *const char,
    library_version: *const char,
    create(services: *const RVService) -> *void,
    destroy(user_data: *void) -> i32,
    output_targets_info(user_data: *void) -> OutputTargets,
    start(user_data: *void, callback: *PlaybackCallback),
    stop(user_data: *void),
    static_init(services: *const RVService),
}
```

(`PlaybackCallback` is a struct with one fn-pointer field — matches the v1 Rust `PlaybackCallback`.)

- [ ] **Step 4: Write `api/resample.def`**

```
#[module(retrovert)]

extern opaque RVService
extern opaque RVSettings

enum ResampleSettingsUpdate {
    Default = 0,
    RequireRestart = 1,
}

#[attributes(Copy)]
struct ConvertConfig {
    input: AudioFormat,
    output: AudioFormat,
}

struct ResamplePlugin {
    api_version: u64,
    name: *const char,
    version: *const char,
    library_version: *const char,
    create(services: *const RVService) -> *void,
    destroy(user_data: *void) -> i32,
    set_config(user_data: *void, format: *const ConvertConfig),
    convert(user_data: *void, output_data: *void, input_data: *void, input_frame_count: u32) -> u32,
    get_expected_output_frame_count(user_data: *void, frame_count: u32) -> u32,
    get_required_input_frame_count(user_data: *void, frame_count: u32) -> u32,
    static_init(services: *const RVService),
    settings_updated(user_data: *void, settings: *const RVSettings) -> ResampleSettingsUpdate,
}
```

Note: `SettingsUpdate` is already defined in `playback.def` within the same `retrovert` module; to avoid a duplicate-type error across defs, `resample.def` uses a locally-named `ResampleSettingsUpdate`. If api_gen resolves cross-def types and rejects the duplicate, replace with a shared single definition (resolve in Step 5). `RVSettings` is `extern opaque` (the v1 `ResamplePlugin.settings_updated` takes `*const SettingsFFI`).

- [ ] **Step 5: (deferred to Task 3 Step 2) — `.def` parse errors are surfaced by the first generator run.** No standalone test here; the `.def` is validated when api_gen parses it in Task 3.

- [ ] **Step 6: Commit the `.def` files**

```bash
cd ~/code/projects/retrovert/repos/retrovert_api
git checkout -b api-gen-defs
git add api/audio_format.def api/playback.def api/output.def api/resample.def
git commit -m "feat(api): add api_gen .def source for the v2 plugin ABI"
```

---

### Task 3: Add the codegen bin to the `retrovert` app and run it

**Repo:** `~/code/projects/retrovert/repos/retrovert` (the app).

**Files:**
- Create: `tools/abi_gen/Cargo.toml`, `tools/abi_gen/src/main.rs`

**Interfaces:**
- Consumes: `api_gen::{Config, generate, naming::Naming}` from Task 1.
- Produces: generated Rust in `retrovert-core/plugin_types/src/generated/` and C headers in a scratch dir.

- [ ] **Step 1: Confirm Flowi's path relative to the app, set the dependency**

Verify where Flowi actually lives and that the app's existing flowi path-dep resolves:

```bash
ls -d ~/code/projects/flowi/rust/tools/api_gen
ls -d ~/code/projects/retrovert/repos/retrovert/../../flowi 2>/dev/null || echo "app's ../../flowi does NOT resolve — use absolute path"
```
Use an **absolute** path dep for this dev-only tool (unambiguous regardless of layout): `api_gen = { path = "/home/emoon/code/projects/flowi/rust/tools/api_gen" }`. (Relativize later if desired; tracked in the design doc follow-ups.)

- [ ] **Step 2: Write `tools/abi_gen/Cargo.toml`**

```toml
[package]
name = "abi_gen"
version = "0.1.0"
edition = "2021"

[dependencies]
api_gen = { path = "/home/emoon/code/projects/flowi/rust/tools/api_gen" }
```

- [ ] **Step 3: Write `tools/abi_gen/src/main.rs`**

```rust
//! Dev-only: regenerate Retrovert's plugin ABI bindings via Flowi's api_gen.
//! Flowi-free consumers (retrovert_api, retrovert-core) only hold the committed output.
//! Run from the `retrovert` repo root: `cargo run --manifest-path tools/abi_gen/Cargo.toml`

use api_gen::{generate, naming::Naming, Config};
use std::path::PathBuf;

fn main() {
    // Sibling repos under ~/code/projects/retrovert/repos/.
    let repos = PathBuf::from("/home/emoon/code/projects/retrovert/repos");
    let retrovert_api = repos.join("retrovert_api");
    let plugin_types = repos.join("retrovert-core/plugin_types");

    generate(&Config {
        naming: Naming::new("RV", "rv"),
        api_dir: retrovert_api.join("api"),
        // Rust FFI lands in plugin_types as a `generated/` module.
        rust_dir: plugin_types.join("src/generated"),
        // C headers to a scratch dir for milestone 1 (diff before overwriting the master).
        c_include_root: PathBuf::from("/tmp/retrovert_api_gen_c"),
        facade_dir: None,
    });
    println!("done. C scratch: /tmp/retrovert_api_gen_c ; Rust: {}", plugin_types.join("src/generated").display());
}
```

- [ ] **Step 4: Run the generator (this is the `.def` syntax test)**

```bash
cd ~/code/projects/retrovert/repos/retrovert
cargo run --manifest-path tools/abi_gen/Cargo.toml
```
Expected: prints `api_gen: generated 4 module(s) ...` and `done.`. If it **panics on a parse error**, fix the offending `.def` line in `retrovert_api/api/*.def` (per the cheat-sheet) and re-run until it generates. Resolve the open `.def` questions here: duplicate `SettingsUpdate` across defs, unused `extern opaque RVChannelInfo`, and the `*const *const char` spelling for `get_scope_channel_names`/`OutputTargets.names`.

- [ ] **Step 5: Inspect the generated Rust against the v2 C ABI**

```bash
ls /home/emoon/code/projects/retrovert/repos/retrovert-core/plugin_types/src/generated/
cat /home/emoon/code/projects/retrovert/repos/retrovert-core/plugin_types/src/generated/playback.rs
```
Expected: `PlaybackPlugin` has the **19 fn-pointer slots** including the 6 viz slots (`get_tracker_info` … `get_scope_channel_names`), `ReadData` has **no** `virtual_channel*` fields, and there is **no** `PlaybackType`/`PlaybackInfo` — i.e. it matches `playback.h`, not the stale v1 `ffi_gen.rs`.

- [ ] **Step 6: Commit the codegen tool (app repo)**

```bash
cd ~/code/projects/retrovert/repos/retrovert
git checkout -b abi-gen-tool
git add tools/abi_gen/Cargo.toml tools/abi_gen/src/main.rs
git commit -m "feat(tools): add abi_gen codegen bin driving flowi api_gen"
```

---

### Task 4: Wire the generated module into `plugin_types` and fix `core` call sites

**Repo:** `~/code/projects/retrovert/repos/retrovert-core`.

**Files:**
- Modify: `plugin_types/src/lib.rs` (swap `ffi_gen` → `generated`)
- Delete: `plugin_types/src/ffi_gen.rs`
- Modify: `core/src/plugin_handler.rs`, `core/src/playback.rs` (Service rename, `Option`-wrapped fn pointers, version constants)

**Interfaces:**
- Consumes: `plugin_types::generated::*` (`PlaybackPlugin`, `OutputPlugin`, `ResamplePlugin`, `AudioFormat`, `ReadInfo`, `ReadData`, `ProbeResult`, `RVService`).

- [ ] **Step 1: Point `plugin_types` at the generated module**

Edit `plugin_types/src/lib.rs` — replace `pub mod ffi_gen; pub use ffi_gen::*;` with:

```rust
pub mod generated;
pub use generated::*;
```

Then delete the stale file:

```bash
cd ~/code/projects/retrovert/repos/retrovert-core
git rm plugin_types/src/ffi_gen.rs
```

- [ ] **Step 2: Add the hand-written version constants**

api_gen does not emit version `#define`/`const`. Add them where `plugin_types` previously had them. Create `plugin_types/src/versions.rs`:

```rust
//! Hand-written API version constants (api_gen does not generate these).
pub const RV_PLAYBACK_PLUGIN_API_VERSION: u64 = 2;
pub const RV_OUTPUT_PLUGIN_API_VERSION: u64 = 1;
pub const RV_RESAMPLE_PLUGIN_API_VERSION: u64 = 1;
```

Add `pub mod versions; pub use versions::*;` to `plugin_types/src/lib.rs`.

- [ ] **Step 3: Build `plugin_types` alone to shake out type errors**

```bash
cd ~/code/projects/retrovert/repos/retrovert-core
cargo build -p plugin_types
```
Expected: compiles. If `RVService` (opaque ZST) or `SettingsUpdate` collide with the `services` crate, note them — they're resolved at the `core` call sites (next step), since `plugin_types` itself only needs the pointer/by-value types it generated.

- [ ] **Step 4: Fix `core` call sites — Service pointer cast**

`core` builds a `services::ServiceFFI` and passes it to plugin `create`/`open`/`static_init`/`metadata`/`settings_updated`, which now take `*const plugin_types::RVService` (opaque). At each call site, cast the service pointer. Example in `core/src/playback.rs` (the resample `create` call, currently `op.plugin_funcs.create)(service_funcs)`):

```rust
let user_data = unsafe {
    (op.plugin_funcs.create)(service_funcs as *const _ as *const plugin_types::RVService)
};
```
Apply the same `as *const _ as *const plugin_types::RVService` cast at every `create`/`open`/`static_init`/`metadata`/`settings_updated` call in `core/src/playback.rs` and `core/src/plugin_handler.rs`. (Find them: `grep -rn 'static_init\|\.create)\|\.open)\|\.metadata)\|settings_updated' core/src`.)

- [ ] **Step 5: Fix `core` call sites — `Option`-wrapped fn pointers**

Generated fn-pointer fields are `Option<extern "C" fn...>`. Direct calls like `(p.create)(x)` must become `(p.create.unwrap())(x)`, and the existing null-check `if plugin_funcs.static_init as usize != 0` becomes `if let Some(f) = plugin_funcs.static_init`. Example in `core/src/plugin_handler.rs`:

```rust
if let Some(static_init) = plugin_funcs.static_init {
    unsafe { static_init(service.get_c_api() as *const _ as *const plugin_types::RVService); }
}
```
For required slots (`create`, `probe_can_play`, `read_data`, `destroy`, `seek`, `close`), use `.unwrap()` (a null required slot is a broken plugin — failing loudly is correct). For the optional viz slots, leave them unread for now (load+play doesn't call them).

- [ ] **Step 6: Update version checks to v2**

Wherever `core` validates `api_version` against `RV_PLAYBACK_PLUGIN_API_VERSION`, it now compares to `2` via the constant from Step 2. Confirm: `grep -rn 'API_VERSION' core/src`. No literal `1` should remain for the playback check.

- [ ] **Step 7: Build the whole workspace**

```bash
cd ~/code/projects/retrovert/repos/retrovert-core
cargo build
```
Expected: the workspace compiles (`core`, `plugin_types`, `services`, `vfs`). Fix remaining type/signature mismatches against the generated structs until clean. The Rust `size_of`/`offset_of` static-asserts api_gen emits in `generated/*.rs` must compile — they are the layout guard.

- [ ] **Step 8: Commit the wiring**

```bash
cd ~/code/projects/retrovert/repos/retrovert-core
git checkout -b api-gen-v2
git add plugin_types/src/ core/src/
git commit -m "feat(core): regenerate plugin_types to v2 ABI via api_gen, fix call sites"
```

---

### Task 5: Verify — C-surface diff and the decisive load-and-play

**Repos:** `retrovert_api` (diff), `retrovert-core` (run).

- [ ] **Step 1: Diff the generated C surface against the v2 master**

```bash
diff <(sed -n '/RVPlaybackPlugin/,/} RVPlaybackPlugin;/p' /home/emoon/code/projects/retrovert/repos/retrovert_api/include/retrovert/playback.h) \
     <(sed -n '/RVPlaybackPlugin/,/} RVPlaybackPlugin;/p' /tmp/retrovert_api_gen_c/retrovert/playback.h)
```
Expected: only cosmetic differences (whitespace/comments, `int`→`int32_t`, fn-pointer arg names). **No structural differences** (field count, order, or types). Investigate any real difference — it means the `.def` mis-describes the ABI.

- [ ] **Step 2: Build a console runner and load a real plugin**

Build `retrovert-console` (or the existing driver) and point it at a compiled v2 plugin + a sample module file:

```bash
cd ~/code/projects/retrovert/repos/retrovert-core
cargo build -p retrovert-console 2>/dev/null || cargo build
# Run the console against a known plugin dir + file (exact invocation per retrovert-console's args):
# e.g. ./target/debug/retrovert-console --plugins <plugin-dir> <some-module-file>
```
Expected: the console loads the playback plugin (its `api_version` reads as `2`, `probe_can_play` accepts the file, `open` + `read_data` return `ReadStatus::Ok` frames).

- [ ] **Step 3: Confirm end-to-end playback**

With an output plugin present, confirm audio plays to completion (or, if output isn't wired yet, confirm `read_data` decodes a non-trivial number of `Ok` frames as the load-correctness proof).
Expected: a file plays / decodes without a crash or garbage — proving the regenerated v2 vtable is byte-compatible with the shipped plugin.

- [ ] **Step 4: Promote C headers and commit (only after diff + run pass)**

Repoint `abi_gen`'s `c_include_root` from the scratch dir to the real `retrovert_api/include`, regenerate, and commit the now-generated headers in `retrovert_api`:

```bash
# edit tools/abi_gen/src/main.rs: c_include_root -> retrovert_api.join("include")
cd ~/code/projects/retrovert/repos/retrovert
cargo run --manifest-path tools/abi_gen/Cargo.toml
cd ~/code/projects/retrovert/repos/retrovert_api
git add api/ include/
git commit -m "feat(api): generate v2 C headers from .def via api_gen"
# then run update-api-headers.sh to fan out to playback_plugins as usual
```

- [ ] **Step 5: Final commit of generated Rust (retrovert-core)**

```bash
cd ~/code/projects/retrovert/repos/retrovert-core
git add plugin_types/src/generated/
git commit -m "chore(plugin_types): commit regenerated v2 FFI bindings"
```

---

## Self-review notes

- **Spec coverage:** Task 1 = lib+bin refactor (§Design 1); Task 2 = `.def` in retrovert_api (§2, §3); Task 3 = codegen bin in the app (§2a); Task 4 = flowi-free consumer wiring + the Service/Option/version reconciliation flagged in the design; Task 5 = the two-check proof (§Proof of done).
- **Known unknowns resolved at runtime (each has an explicit fix step, not a placeholder):** exact `.def` syntax edge cases (Task 3 Step 4), `generate_mod_file` output shape for a non-flowi tree (Task 4 Step 3/7 via `cargo build`), and the app's flowi path (Task 3 Step 1).
- **Deferred (not in this plan):** fixed-array codegen, Direction-2 service generation, the tracker/scope redesign — see the design doc's "Out of scope."
