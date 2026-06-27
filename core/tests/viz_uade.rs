// dlopen the real uade plugin and drive the value-semantic viz vtable end to end,
// proving the scope-only model: advertises Scope but no PatternCells, leaves every
// pattern getter NULL, reports the four Amiga Paula scope channels, and returns
// non-silent scope only after an explicit set_scope_enabled (no hidden auto-on).
// Skips (does not fail) if either the plugin .so or the test module is missing.

use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use plugin_types::{
    AudioFormat, AudioStreamFormat, ChannelDesc, PlaybackPlugin, ReadData, ReadInfo, ReadStatus,
    RVService, ScrollMode, VizCaps, VizStructure,
};
use services::PluginService;
use std::path::PathBuf;
use vfs::Vfs;

fn so_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_UADE_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-uade/build/plugins/uade_playback.so")
}

fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_UADE_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/uade/expressions.bp")
}

fn render(plugin: &PlaybackPlugin, user_data: *mut std::ffi::c_void, reads: usize) {
    let mut buf = vec![0f32; 1024 * 2];
    for _ in 0..reads {
        let rd = ReadData {
            channels_output: buf.as_mut_ptr() as _,
            channels_output_max_bytes_size: (buf.len() * 4) as u32,
            info: ReadInfo {
                format: AudioFormat { audio_format: AudioStreamFormat::S16, channel_count: 2, sample_rate: 48000 },
                frame_count: 0,
                status: ReadStatus::DecodingRequest,
            },
        };
        (plugin.read_data.unwrap())(user_data, rd);
    }
}

#[test]
fn uade_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built", path.display());
        return;
    }
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: test module {} not found (set RV_UADE_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("uade-test", Vfs::new());
    let svc = service.get_c_api() as *const RVService;

    let lib = unsafe { Library::new(&path) }.expect("dlopen");
    let entry: Symbol<extern "C" fn() -> *const PlaybackPlugin> =
        unsafe { lib.get(b"rv_playback_plugin\0") }.expect("rv_playback_plugin");
    let plugin = unsafe { &*entry() };

    assert_eq!(plugin.api_version, plugin_types::RV_PLAYBACK_PLUGIN_API_VERSION);

    (plugin.static_init.unwrap())(svc);
    let user_data = (plugin.create.unwrap())(svc);
    assert!(!user_data.is_null());

    let c_url = CFixedString::from_str(module.to_str().unwrap());
    let rc = (plugin.open.unwrap())(user_data, c_url.as_ptr(), 0, svc);
    assert_eq!(rc, 0, "open failed for {}", module.display());

    render(plugin, user_data, 4);

    // --- structure: scope-only — Scope, no PatternCells ---
    let mut st = VizStructure { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.get_structure.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "uade must advertise Scope");
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() == 0, "scope-only: must NOT advertise PatternCells");
    assert_eq!(st.pattern_channel_count, 0, "scope-only: no pattern channels");
    assert_eq!(st.column_count, 0, "scope-only: no cell columns");
    assert_eq!(st.scope_channel_count, 4, "uade exposes the four Amiga Paula channels");
    let scope_channels = st.scope_channel_count as usize;

    // --- scope-only contract: pattern getters are NULL ---
    assert!(plugin.get_columns.is_none(), "scope-only: get_columns must be NULL");
    assert!(plugin.get_pattern_channels.is_none(), "scope-only: get_pattern_channels must be NULL");
    assert!(plugin.get_position.is_none(), "scope-only: get_position must be NULL");
    assert!(plugin.get_channel_rows.is_none(), "scope-only: get_channel_rows must be NULL");
    assert!(plugin.get_cells.is_none(), "scope-only: get_cells must be NULL");
    assert!(plugin.get_vu.is_none(), "scope-only: get_vu must be NULL");

    // --- scope channels: four, each named, mono ---
    let mut chans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; scope_channels];
    let nc = (plugin.get_scope_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc as usize, scope_channels, "scope channel count must match structure");
    assert_ne!(chans[0].name[0], 0, "scope channel name should be populated");
    assert_eq!(chans[0].scope_width, 0, "scope-only: mono scope width");

    // --- no hidden auto-on: scope yields nothing until explicitly enabled ---
    render(plugin, user_data, 4);
    let mut scope = vec![0f32; 1024];
    let off = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(off, 0, "scope must be silent before set_scope_enabled(true)");

    // --- scope: non-silent samples after explicit enable on a known file ---
    (plugin.set_scope_enabled.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let mut found_peak = 0f32;
    for ch in 0..scope_channels as i32 {
        let ns = (plugin.get_scope_samples.unwrap())(user_data, ch, scope.as_mut_ptr(), scope.len() as u32);
        if ns > 0 {
            let p = scope[..ns as usize].iter().fold(0f32, |a, &x| a.max(x.abs()));
            found_peak = found_peak.max(p);
        }
    }
    assert!(found_peak > 1e-4, "scope is silent across all channels (peak {found_peak})");

    (plugin.destroy.unwrap())(user_data);
}
