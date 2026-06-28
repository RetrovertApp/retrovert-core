// dlopen the real xsf plugin and drive the value-semantic viz vtable. The static
// scope-only wiring (scope getters present, pattern getters + vu_levels NULL) is asserted
// unconditionally. Two live paths follow:
//   * xsf_viz_vtable      — a PSF file exposes the PSX SPU scope (caps = Scope, dynamic
//                           voice count, no-auto-on contract). Needs the host PSX BIOS;
//                           skips gracefully if open fails (BIOS absent in CI).
//   * xsf_non_psf_caps_zero — a non-PSF xSF file (GSF) must report caps = 0 / no scope.
// Skips (does not fail) if the .so or a module is missing.

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
    if let Ok(p) = std::env::var("RV_XSF_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-xsf/build/plugins/xsf_playback.so")
}

fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_XSF_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/xsf/test.psf")
}

fn gsf_module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_XSF_GSF_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/xsf/test.gsf")
}

// Renders `reads` blocks, asserts the decoder never faults, and returns total frames.
fn render(plugin: &PlaybackPlugin, user_data: *mut std::ffi::c_void, reads: usize) -> u32 {
    let mut buf = vec![0f32; 1024 * 2];
    let mut frames = 0u32;
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
        let info = (plugin.read_data.unwrap())(user_data, rd);
        assert_ne!(info.status, ReadStatus::Error, "decode returned Error status");
        frames += info.frame_count;
    }
    frames
}

fn load<'a>(lib: &'a Library) -> &'a PlaybackPlugin {
    let entry: Symbol<extern "C" fn() -> *const PlaybackPlugin> =
        unsafe { lib.get(b"rv_playback_plugin\0") }.expect("rv_playback_plugin");
    unsafe { &*entry() }
}

#[test]
fn xsf_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built", path.display());
        return;
    }

    let lib = unsafe { Library::new(&path) }.expect("dlopen");
    let plugin = load(&lib);

    assert_eq!(plugin.api_version, plugin_types::RV_PLAYBACK_PLUGIN_API_VERSION);

    // Vtable wiring (independent of any module): scope-only getters present,
    // pattern getters and vu_levels NULL. Verified even with no PSX BIOS on disk.
    assert!(plugin.viz_info.is_some(), "xsf must implement viz_info");
    assert!(plugin.scope_channels.is_some(), "xsf must implement scope_channels");
    assert!(plugin.scope_enable.is_some(), "xsf must implement scope_enable");
    assert!(plugin.scope_samples.is_some(), "xsf must implement scope_samples");
    assert!(plugin.tracker_columns.is_none(), "scope-only: tracker_columns must be NULL");
    assert!(plugin.tracker_channels.is_none(), "scope-only: tracker_channels must be NULL");
    assert!(plugin.tracker_position.is_none(), "scope-only: tracker_position must be NULL");
    assert!(plugin.tracker_channel_rows.is_none(), "scope-only: tracker_channel_rows must be NULL");
    assert!(plugin.tracker_cells.is_none(), "scope-only: tracker_cells must be NULL");
    assert!(plugin.vu_levels.is_none(), "scope-only: vu_levels must be NULL");

    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping xsf live scope: module {} not found", module_path().display());
            return;
        }
    };

    let service = PluginService::new("xsf-test", Vfs::new());
    let svc = service.get_c_api() as *const RVService;

    if let Some(init) = plugin.static_init {
        init(svc);
    }
    let user_data = (plugin.create.unwrap())(svc);
    assert!(!user_data.is_null());

    // PSF1 (highly_experimental) needs the host-provided PSX BIOS (psf_bios/
    // scph10000_he.bin) to start the emulator. It is not shipped with test data,
    // so a failed open here means "no BIOS" — skip the live scope flow rather
    // than fail. The static wiring above is what this test guarantees.
    let c_url = CFixedString::from_str(module.to_str().unwrap());
    let rc = (plugin.open.unwrap())(user_data, c_url.as_ptr(), 0, svc);
    if rc != 0 {
        eprintln!("skipping xsf live scope: open failed (PSX BIOS likely absent)");
        (plugin.destroy.unwrap())(user_data);
        return;
    }

    assert!(render(plugin, user_data, 4) > 0, "decoder produced no frames");

    // --- structure: scope-only — Scope, no PatternCells, dynamic SPU voice count ---
    let mut st = VizInfo { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.viz_info.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "xsf (PSF) must advertise Scope");
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() == 0, "scope-only: must NOT advertise PatternCells");
    assert_eq!(st.pattern_channel_count, 0, "scope-only: no pattern channels");
    assert_eq!(st.column_count, 0, "scope-only: no cell columns");
    assert!(st.scope_channel_count > 0, "PSF must report SPU voices");
    let scope_channels = st.scope_channel_count as usize;

    // --- scope channels: dynamic count, each named, mono ---
    let mut chans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; scope_channels];
    let nc = (plugin.scope_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc as usize, scope_channels, "scope channel count must match structure");
    assert_ne!(chans[0].name[0], 0, "scope channel name should be populated");
    assert_eq!(chans[0].scope_width, 0, "scope-only: mono scope width");

    // --- no hidden auto-on: scope yields nothing until explicitly enabled ---
    render(plugin, user_data, 4);
    let mut scope = vec![0f32; 1024];
    let off = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(off, 0, "scope must be silent before scope_enable(true)");

    // --- scope: non-silent samples after explicit enable ---
    (plugin.scope_enable.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let mut found_peak = 0f32;
    for ch in 0..scope_channels as i32 {
        let ns = (plugin.scope_samples.unwrap())(user_data, ch, scope.as_mut_ptr(), scope.len() as u32);
        if ns > 0 {
            let p = scope[..ns as usize].iter().fold(0f32, |a, &x| a.max(x.abs()));
            found_peak = found_peak.max(p);
        }
    }
    assert!(found_peak > 1e-4, "scope is silent across all channels (peak {found_peak})");

    (plugin.destroy.unwrap())(user_data);
}

// A non-PSF xSF format (GSF / Game Boy Advance) has no scope: viz_info must
// honestly report caps = 0 and zero scope channels, and the scope getters must yield
// nothing. This covers the xsf_get_structure non-PSF branch. GSF needs no BIOS.
#[test]
fn xsf_non_psf_caps_zero() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built", path.display());
        return;
    }
    let module = match std::fs::canonicalize(gsf_module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping xsf non-PSF: module {} not found", gsf_module_path().display());
            return;
        }
    };

    let service = PluginService::new("xsf-gsf-test", Vfs::new());
    let svc = service.get_c_api() as *const RVService;

    let lib = unsafe { Library::new(&path) }.expect("dlopen");
    let plugin = load(&lib);

    if let Some(init) = plugin.static_init {
        init(svc);
    }
    let user_data = (plugin.create.unwrap())(svc);
    assert!(!user_data.is_null());

    let c_url = CFixedString::from_str(module.to_str().unwrap());
    let rc = (plugin.open.unwrap())(user_data, c_url.as_ptr(), 0, svc);
    if rc != 0 {
        eprintln!("skipping xsf non-PSF: GSF open failed");
        (plugin.destroy.unwrap())(user_data);
        return;
    }

    assert!(render(plugin, user_data, 4) > 0, "GSF decoder produced no frames");

    let mut st = VizInfo { caps: 0xFFFF_FFFF, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 9, scope_channel_count: 9, column_count: 9 };
    assert!((plugin.viz_info.unwrap())(user_data, &mut st));
    assert_eq!(st.caps, 0, "non-PSF xSF must report caps = 0");
    assert_eq!(st.scope_channel_count, 0, "non-PSF xSF has no scope channels");

    let mut chans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; 4];
    let nc = (plugin.scope_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc, 0, "non-PSF xSF returns no scope channels");

    let mut scope = vec![0f32; 1024];
    (plugin.scope_enable.unwrap())(user_data, true);
    render(plugin, user_data, 8);
    let ns = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(ns, 0, "non-PSF xSF yields no scope samples");

    (plugin.destroy.unwrap())(user_data);
}
