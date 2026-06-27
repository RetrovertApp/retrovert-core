# api_gen: support fixed-size array struct fields

Status: done
Type: AFK
Est. context: ~70k

## Parent

Tracker/Scope redesign — `.scratch/tracker-scope-redesign/PRD.md`; design-of-record `docs/adr/0002-visualization-abi-is-value-semantic.md`. This is the enabling first slice (codegen-first).

## What to build

Implement Flowi `api_gen`'s deferred support for **fixed-size array fields inside structs** (e.g. `name: [u8; 24]`), so the value-semantic viz structs (ADR-0002) can be code-generated into both C and Rust. The grammar already parses `[T; N]` and captures the size; the layout pass currently bails on any array field and the two generators don't emit the `[N]` suffix for fields. Make a struct with a fixed-array field of a primitive type round-trip: correct size/alignment in the layout pass, a `type name[N];` C field, a `[T; N]` Rust field, and the existing generated size/offset static-asserts must hold.

Scope note: only **numeric-literal** array sizes need to work (e.g. `[u8; 16]`). Symbolic sizes (a named constant) are out of scope — there is no constant-resolution pass and the viz `.def` will use literals; the hand-authored C header keeps the `RV_CELL_TEXT_MAX` macro. Per ADR-0001 this lands in the Flowi `api_gen` repo; the retrovert consumers stay flowi-free.

## Acceptance criteria

- [x] A struct with a fixed-size array field of a primitive (e.g. `{ name: [u8; 24], scope_width: u8 }`) lays out with size = `elem_size * N` (+ trailing field), alignment = element alignment, no panic.
- [x] Generated C emits the field as `uint8_t name[24];`.
- [x] Generated Rust emits the field as `pub name: [u8; 24],` with passing `size_of`/`offset_of` static-asserts.
- [x] A regression test (inline `#[test]`, matching api_gen's existing test style) covers a fixed-array-field struct in both backends and fails if the layout/emit regresses.
- [x] Existing api_gen output for current `.def`s regenerates byte-identical (no collateral change).

## Blocked by

- None — can start immediately.

## Comments

### 2026-06-27 — Implemented
- Commit: 5083220 api_gen: support fixed-size array struct fields (flowi repo, per ADR-0001)
- Reviewers: 1 (combined-all: correctness + quality + test-coverage)
- Rounds: 1, 0 findings across the loop
- Note: change lands in `flowi/rust/tools/api_gen`; retrovert tracker dir is untracked so this issue file is updated on disk, not committed.
