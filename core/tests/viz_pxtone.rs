// dlopen the real pxtone plugin and drive the value-semantic viz vtable end to end,
// proving the stereo-scope + VU model: a plugin that advertises Scope | Vu, reports
// scope_width = 2 per channel, returns interleaved L/R samples, exposes an explicit
// on/off scope switch (no hidden auto-on), and reports per-channel VU levels.
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
    if let Ok(p) = std::env::var("RV_PXTONE_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-pxtone/build/plugins/pxtone_playback.so")
}

fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_PXTONE_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/pxtone/test.ptcop")
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
fn pxtone_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-pxtone)", path.display());
        return;
    }
    // Canonicalize: the VFS does not resolve `..` components in a path.
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: pxtone test module {} not found (set RV_PXTONE_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("pxtone-test", Vfs::new());
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

    // --- structure: scope + VU, no pattern grid ---
    let mut st = VizStructure { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.get_structure.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "pxtone must advertise Scope");
    assert!(st.caps & VizCaps::VU.bits() != 0, "pxtone must advertise Vu");
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() == 0, "scope-only: must NOT advertise PatternCells");
    assert_eq!(st.scroll_mode, ScrollMode::PerChannel);
    assert_eq!(st.pattern_channel_count, 0, "scope-only: no pattern channels");
    assert_eq!(st.column_count, 0, "scope-only: no cell columns");
    assert!(st.scope_channel_count > 0, "must report at least one scope voice");
    let scope_channels = st.scope_channel_count as usize;

    // --- scope-only contract: pattern getters are NULL ---
    assert!(plugin.get_columns.is_none(), "scope-only: get_columns must be NULL");
    assert!(plugin.get_pattern_channels.is_none(), "scope-only: get_pattern_channels must be NULL");
    assert!(plugin.get_position.is_none(), "scope-only: get_position must be NULL");
    assert!(plugin.get_channel_rows.is_none(), "scope-only: get_channel_rows must be NULL");
    assert!(plugin.get_cells.is_none(), "scope-only: get_cells must be NULL");

    // --- scope channels: each named, every voice declares stereo width 2 ---
    let mut chans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; scope_channels];
    let nc = (plugin.get_scope_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc as usize, scope_channels, "scope channel count must match structure");
    assert_ne!(chans[0].name[0], 0, "scope channel name should be populated");
    assert!(chans.iter().all(|c| c.scope_width == 2), "every scope channel must report stereo width 2");

    // --- no hidden auto-on: with capture never enabled, scope reads return nothing ---
    render(plugin, user_data, 4);
    let mut scope = vec![0f32; 2048];
    let ns_off = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(ns_off, 0, "scope must stay silent until set_scope_enabled(true)");

    // --- enable: interleaved stereo samples appear, count is even (L/R pairs) ---
    (plugin.set_scope_enabled.unwrap())(user_data, true);
    render(plugin, user_data, 20);

    let mut found_stereo_separation = false;
    let mut max_peak = 0f32;
    for ch in 0..scope_channels {
        let mut buf = vec![0f32; 2048];
        let n = (plugin.get_scope_samples.unwrap())(user_data, ch as i32, buf.as_mut_ptr(), buf.len() as u32) as usize;
        assert_eq!(n % 2, 0, "stereo scope must return an even (interleaved) sample count");
        let mut i = 0;
        while i + 1 < n {
            let (l, r) = (buf[i], buf[i + 1]);
            max_peak = max_peak.max(l.abs()).max(r.abs());
            if (l - r).abs() > 1e-4 {
                found_stereo_separation = true;
            }
            i += 2;
        }
    }
    assert!(max_peak > 1e-4, "scope is silent after enable (peak {max_peak})");
    assert!(found_stereo_separation, "no L/R separation found — stereo interleaving not proven");

    // --- VU: per-channel levels present while enabled ---
    let mut vu = vec![0f32; scope_channels];
    let nv = (plugin.get_vu.unwrap())(user_data, vu.as_mut_ptr(), vu.len() as u32);
    assert_eq!(nv as usize, scope_channels, "VU must report one level per scope channel");
    assert!(vu.iter().any(|&v| v > 1e-4), "VU is silent on an audible file");

    // --- disable stops capture; re-enable resumes ---
    (plugin.set_scope_enabled.unwrap())(user_data, false);
    render(plugin, user_data, 5);
    let ns_disabled = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(ns_disabled, 0, "set_scope_enabled(false) must stop capture");

    (plugin.set_scope_enabled.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let ns_resumed = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert!(ns_resumed > 0, "set_scope_enabled(true) must resume capture");

    (plugin.destroy.unwrap())(user_data);
}
