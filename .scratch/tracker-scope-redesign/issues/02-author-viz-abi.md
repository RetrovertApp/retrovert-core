# Author the new visualization ABI (single-source playback.def → gen C + Rust); clean-break fan-out

Status: done
Type: AFK
Est. context: ~70k

## Parent

Tracker/Scope redesign PRD; design-of-record `docs/adr/0002-visualization-abi-is-value-semantic.md`.

## What was built (decisions diverged from original plan)

Two user-confirmed changes from the original framing:
- **Clean break, not additive.** The legacy tracker/scope slots, `RVPatternCell`, and the `tracker_legacy` shim were deleted, not kept. `RV_PLAYBACK_PLUGIN_API_VERSION` bumped to 2. All 28 plugins migrate (rolled out per #04+).
- **Single-source `.def`, not a hand-authored C master.** `retrovert_api/api/playback.def` is the one source; api_gen generates **both** `retrovert_api/include/retrovert/playback.h` and `retrovert-core/plugin_types/src/generated/playback.rs`. (Reverses the earlier "hand-authored playback.h is the master" decision.)

Surface landed (shapes per ADR-0002):

- Enums: `RVVizCaps` bitset (`PatternCells, Scope, Vu, WholeSongKnown, SeekablePreview, FutureKnown`), `RVScrollMode` (`Synchronized, PerChannel`), `RVColumnKind` (`Note, Instrument, Volume, Effect, Param, Custom`).
- Value-semantic structs (inline fixed arrays, no pointers/strings): `RVVizStructure`, `RVColumnDesc { label[16], char_width, kind }`, `RVChannelDesc { name[24], scope_width }`, `RVVizPosition`, `RVCell { raw, text[16] }`. `RV_CELL_TEXT_MAX` = 16.
- New `RVPlaybackPlugin` vtable slots: `get_structure` (out-ptr+bool), `get_columns`/`get_pattern_channels`/`get_scope_channels` (typed ptr+cap, return count), `get_position` (out-ptr+bool), `get_channel_rows`, `get_cells(channel, row_lo, row_hi, ...)`, `set_scope_enabled(bool)`, `get_scope_samples(channel, ...)`, `get_vu(...)`.
- `output_frame` is **not** in `RVVizPosition` — the host stamps the frame on the snapshot (see #05), plugins don't track it.

## Acceptance criteria

- [x] New enums/structs/vtable slots present in `playback.def`; legacy slots/`RVPatternCell`/`tracker_legacy` deleted (clean break).
- [x] Rust bindings regenerate with `RVVizStructure`/`RVColumnDesc`/`RVChannelDesc`/`RVVizPosition`/`RVCell` as real `#[repr(C)]` structs (not opaque ZST enums), with passing size/offset asserts.
- [x] C header (generated) and the generated Rust layouts agree (size/offset asserts consistent; openmpt + harness link and run against them — #03/#04).
- [x] `update-api-headers.sh` fans the header to all 28 plugins.
- [x] retrovert_api/retrovert-core remain flowi-free (no `<flowi/...>` leaks; C uses flat includes + `extern "C"`).

## Blocked by

- #01 (api_gen fixed-array support) — must be present in the same Flowi checkout used to regenerate, or the regen panics on the inline-array fields. Requires the four-repo side-by-side layout (flowi + retrovert app + retrovert_api + retrovert-core).
