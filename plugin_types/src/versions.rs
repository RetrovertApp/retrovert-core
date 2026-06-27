//! Hand-written API version constants for defs that don't yet carry a
//! `#[c_define]`. Playback's version is now generated (single source in
//! playback.def) and re-exported via `generated::playback`, so it's gone here.
pub const RV_OUTPUT_PLUGIN_API_VERSION: u64 = 1;
pub const RV_RESAMPLE_PLUGIN_API_VERSION: u64 = 1;
