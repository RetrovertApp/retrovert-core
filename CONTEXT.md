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
