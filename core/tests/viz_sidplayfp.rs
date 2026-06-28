// dlopen the real sidplayfp plugin and drive the value-semantic viz vtable end to end,
// proving the metadata-only + scope model: a plugin that advertises Scope but no
// PatternCells, leaves every pattern getter NULL, and reports a dynamic scope channel
// count that exceeds the old RV_MAX_CHANNELS=8 for a multi-SID tune (3 SIDs * 3 = 9).
// Skips (does not fail) if either the plugin .so or the test module is missing.

use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use plugin_types::{
    AudioFormat, AudioStreamFormat, ChannelDesc, PlaybackPlugin, ReadData, ReadInfo, ReadStatus,
    RVService, ScrollMode, VizCaps, VizInfo,
};
use services::PluginService;
use std::path::PathBuf;
use vfs::Vfs;

fn so_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_SIDPLAYFP_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-sidplayfp/build/plugins/sidplayfp_playback.so")
}

// Canonical known module: Devils ReSIDence, a 3-SID tune (9 voices) — the only way to
// exercise the >8 scope channel count the old fixed RV_MAX_CHANNELS=8 could not express.
// Override with RV_SID_MODULE to point at any local .sid.
fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_SID_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/sidplayfp/Devils_ReSIDence_3SID.sid")
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
fn sidplayfp_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-sidplayfp)", path.display());
        return;
    }
    // Canonicalize: the VFS does not resolve `..` components in a path.
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: 3-SID test module {} not found (set RV_SID_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("sidplayfp-test", Vfs::new());
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

    // Establish playback so the SID emulators fill their voice buffers.
    render(plugin, user_data, 4);

    // --- structure: metadata-only — Scope, no PatternCells, dynamic count > 8 ---
    let mut st = VizInfo { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.viz_info.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "sidplayfp must advertise Scope");
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() == 0, "metadata-only: must NOT advertise PatternCells");
    assert_eq!(st.pattern_channel_count, 0, "metadata-only: no pattern channels");
    assert_eq!(st.column_count, 0, "metadata-only: no cell columns");
    assert!(st.scope_channel_count > 8, "3-SID tune must report >8 scope voices, got {}", st.scope_channel_count);
    let scope_channels = st.scope_channel_count as usize;

    // --- metadata-only contract: no pattern grid is exposed; the cell getters are NULL,
    // so the host can never request a pattern view (AC #1: no grid, no crash). ---
    assert!(plugin.tracker_columns.is_none(), "metadata-only: tracker_columns must be NULL");
    assert!(plugin.tracker_channels.is_none(), "metadata-only: tracker_channels must be NULL");
    assert!(plugin.tracker_position.is_none(), "metadata-only: tracker_position must be NULL");
    assert!(plugin.tracker_channel_rows.is_none(), "metadata-only: tracker_channel_rows must be NULL");
    assert!(plugin.tracker_cells.is_none(), "metadata-only: tracker_cells must be NULL");

    // --- scope channels: dynamic count > 8, each named, caller buffer sized from structure ---
    let mut chans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; scope_channels];
    let nc = (plugin.scope_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc as usize, scope_channels, "scope channel count must match structure");
    assert_ne!(chans[0].name[0], 0, "scope channel name should be populated");
    assert_ne!(chans[scope_channels - 1].name[0], 0, "9th voice name should be populated (proves no 8-channel clamp)");

    // --- scope: non-silent samples on voice 0 of a known file ---
    (plugin.scope_enable.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let mut scope = vec![0f32; 1024];
    let ns = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert!(ns > 0, "scope returned no samples");
    let peak = scope[..ns as usize].iter().fold(0f32, |a, &x| a.max(x.abs()));
    assert!(peak > 1e-4, "scope is silent (peak {peak})");

    (plugin.destroy.unwrap())(user_data);
}
