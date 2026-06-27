# 0002 — The visualization ABI is value-semantic (inline fixed arrays, no borrowed pointers)

## Status

accepted (2026-06-27)

## Context

The redesigned tracker/scope visualization surface of the Plugin ABI is queried by the host on the
decode thread, copied into a frame-stamped **snapshot**, and handed to the UI thread (see the
tracker/scope redesign PRD). The strings in this surface — channel names, column labels, and
plugin-rendered cell text — are short and format-bounded. The genuinely long, variable catalog text
(song/sample/artist names) lives in the **metadata API**, not here.

We had to choose how dynamic data (names, cells, scope) crosses the ABI in a way that codegens
cleanly to both C and Rust via Flowi `api_gen`.

## Decision

**Visualization structs carry their data by value through inline fixed-size arrays; plugins fill
caller-owned slices; the host copies structs by value into the snapshot. No borrowed pointers, no
arena.**

- Names/labels/cell text are inline fixed arrays (`RVChannelDesc.name[24]`, `RVColumnDesc.label[16]`,
  `RVCell.text[RV_CELL_TEXT_MAX]`, `RV_CELL_TEXT_MAX = 16`).
- Collections are filled into caller-owned buffers via `api_gen`'s `[slice_mut(T)]` (Rust sees
  `&mut [T]`, C sees `(T*, count)`); the getter returns the count filled.
- **Counts stay dynamic** (`column_count`, pattern/scope channel counts). The old `RV_MAX_CHANNELS`
  fixed *count* is gone. Only per-field **char widths** are bounded (`char_width ≤ RV_CELL_TEXT_MAX`),
  which tracker text trivially satisfies.
- This requires implementing `api_gen`'s deferred fixed-array struct-field support (~30 lines across
  `layout.rs` / `c_gen.rs` / `rust_gen.rs`; the grammar already parses `[T; N]`).

## Considered options

- **`RVString { ptr, len }`, or `api_gen`'s built-in `String` (`FlString`).** Unbounded length and no
  `api_gen` change — but reintroduces a borrowed lifetime (the host must copy each string's bytes out
  before the plugin reuses its buffer), and `FlString` drags a `<flowi/...>` header into the
  deliberately flowi-free `retrovert_api` (ADR-0001). Rejected: the viz surface has **no** unbounded
  strings, so the lifetime cost and the flowi coupling buy nothing.
- **`void*` buffer + host-computed per-cell stride for `get_cells`.** Rejected as an untyped C-ism; a
  fixed-size `RVCell` makes cells a clean typed `[slice_mut(RVCell)]` element instead.

## Consequences

- The snapshot is self-contained plain-old-data: the decode→UI hand-off is a copy with **zero**
  lifetime bookkeeping — no "valid until the next call" footguns.
- `api_gen` gains general fixed-array struct-field support, useful beyond viz (tracked in the
  `api-gen-codegen-gaps` notes). Per ADR-0001 this change lands in the Flowi-side generator; the
  `retrovert_api` / `retrovert-core` consumers stay flowi-free, consuming only committed bindings.
- A future format needing >16 chars of rendered text per column must revisit `RV_CELL_TEXT_MAX`. The
  bound is deliberate and distinct from the `RV_MAX_CHANNELS` mistake: that capped a *count* formats
  genuinely exceed; this caps a *char width* that is inherently tiny.
