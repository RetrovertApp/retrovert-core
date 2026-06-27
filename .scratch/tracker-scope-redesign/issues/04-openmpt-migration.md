# openmpt migration (synchronized random-access) + typed assertions

Status: done
Type: AFK
Est. context: ~60k

## Parent

Tracker/Scope redesign PRD (proving set — synchronized model).

## What to build

Migrate `playback-openmpt` to the new visualization vtable as the first end-to-end proof of the synchronized random-access model. Advertise `caps = PatternCells | Scope | WholeSongKnown`, `scroll_mode = Synchronized`. Implement: `get_structure`, `get_columns` (the MOD/XM/IT column schema: note, instrument, volume, effect, param), `get_pattern_channels`/`get_scope_channels`, `get_position`, `get_cells` (windowed batch; `channel = -1` fills all channels row-major; the whole song is the window), and the new `get_scope_data`. Each cell carries the raw value plus openmpt's own fixed-width rendered text (standardized effect encoding at the plugin boundary). Build the plugin from source (CMake) and verify through the committed harness against a known module.

## Acceptance criteria

- [x] openmpt advertises the correct caps + `Synchronized` scroll mode and a complete column schema.
- [x] `get_cells(channel=-1, …)` returns a window of cells with both raw values and rendered fixed-width text matching the pattern of a known module.
- [x] Pattern + scope channel counts/names are reported (count may exceed the old 8).
- [x] `get_scope_data` returns non-silent float samples on a known-audible file.
- [x] Harness test asserts all of the above through the generated bindings.

## Blocked by

- #03 (committed dlopen viz harness).
