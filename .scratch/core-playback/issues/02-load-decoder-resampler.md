# Load a decoder + resampler plugin

Status: done
Type: AFK
Est. context: ~45k

## What to build

Make the core actually discover and instantiate plugins. The loader must accept `.so`, `.dylib`, and `.rvp` — today it matches only `.rvp`, so nothing loads. Build the `miniaudio_resampler` C plugin and at least one `playback-*` decoder, and make discovery find them (via `--plugins <dir>` and/or a sane default path). The deeper cause of "loads nothing" is an empty plugins dir plus the extension filter — fix both.

Demo target: the console loads one decoder plugin and the resampler, creates an instance of each, and logs them — no panic.

## Acceptance criteria

- [x] Loader accepts `.so`, `.dylib`, and `.rvp` plugin files.
- [x] `miniaudio_resampler` builds and loads as the resample plugin.
- [x] At least one decoder plugin builds and loads; `--plugins <dir>` points discovery at built artifacts.
- [x] Console logs the loaded decoder + resampler and creates instances without panic.

## Blocked by

- 01 (prefactor / static-link)

## Comments

### 2026-06-28 — Implemented
- Commit: 73eccc4 core: 02 load decoder + resampler plugins
- Reviewers: 1 (combined correctness+quality+coverage)
- Rounds: 1, 0 findings (1 non-blocking note: trimmed the `report_loaded` doc comment to match the no-verbose-comments style)
- Notes: `check_file_type` now accepts `rvp`/`so`/`dylib`; `Plugins::report_loaded` logs loaded decoders+resamplers and smoke-creates one instance of each decoder. `create_default_output` now guards the empty output-plugins case (was a `[0]` panic) so the no-output-plugin demo doesn't crash. C plugins built from sibling repos: `miniaudio_resampler` → `repos/bin/plugins/libminiaudio_resampler.rvp` (the default discovery path), `playback-organya` → its `build/plugins/organya_playback.so` (loaded via `--plugins`).
