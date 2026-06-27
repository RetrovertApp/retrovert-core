# Context glossary — Retrovert

Ubiquitous language for the Retrovert music player. Glossary only — no implementation details.

## Terms

**Retrovert** — A modern player for retro music formats (Amiga/C64/console/chiptune),
XMPlay-inspired, deliberately modular. The frontend + core in this project.

**RePlay** — A *separate* frontend project (not an old name for Retrovert). Its UI differs
substantially from Retrovert. Relevant here because it **shares the same Playback Plugins** with
Retrovert: the plugin ABI is a contract common to both, not owned by either frontend alone.

**Playback Plugin** — A dynamically-loaded module (one of the `playback-*` libraries) that decodes
one or more retro music formats. Implements the C plugin ABI defined by `retrovert_api`
(`RVPlaybackPlugin`). Shared by Retrovert and RePlay.

**Plugin ABI** — The C contract a Playback Plugin implements and a host loads against
(`RVPlaybackPlugin`, `RVService`, the `RV*` types, entry symbol `rv_playback_plugin`). Currently at
**v2** for the playback API; the service APIs (io/log/metadata/settings) are at v1. The `RV` prefix
is fixed by the shipped plugins — any regeneration must reproduce it.

**Service** — A host-provided capability a Playback Plugin calls back into (io, log, metadata,
settings), handed to the plugin as a vtable via `RVService`. The host implements these; the plugin
consumes them.

**Flowi** — The in-house Rust+C immediate-mode GUI framework (`~/code/projects/flowi`, formerly
`em_ui`) on which Retrovert's new UI is built. Owns `api_gen`, the IDL→C+Rust code generator.

**RePlay plugin profile (`Rp`)** — A *Flowi* concern, unrelated to Retrovert's Plugin ABI: a
reserved `api_gen` output profile for exposing Flowi's own UI API to RePlay as a host. Does **not**
apply to Retrovert's playback codegen. Named here only to disambiguate it from the `RV` Plugin ABI.

## Visualization

**Visualization API** — The tracker + scope surface of the Plugin ABI: how a Playback Plugin exposes
what it is playing for on-screen display. Capability-driven — a plugin advertises what it supports;
the host never special-cases a format.
_Avoid_: viz (in prose), display API.

**Pattern cell** — The value at one (row, channel, column) of a tracker pattern: a raw value (for
host colouring) plus the plugin's own fixed-width rendered text (for generic layout).
_Avoid_: note (a cell is more than its note), tracker entry.

**Scope** — A per-channel waveform tap of a plugin's audio voices, drawn as an oscilloscope. May be
mono or stereo per channel. Distinct from **VU**, a cheap per-channel level (peak/RMS).
_Avoid_: oscilloscope (UI rendering of a scope), waveform.

**Scrolling mode** — Whether a format's channels advance through rows together (**synchronized**, e.g.
MOD/XM) or each channel scrolls independently (**per-channel**, e.g. TFMX). Advertised by the plugin.
_Avoid_: sync flag, channels_synchronized (the old, ignored field).

**Visualization snapshot** — A coherent, frame-stamped set of visualization data for a single instant,
built by the host on the decode thread and handed to the UI thread. Self-contained by value.
_Avoid_: frame, viz state.
