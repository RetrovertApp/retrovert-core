// dlopen the real ixalance plugin and drive the value-semantic viz vtable end to end,
// proving the full pattern-cell tracker model: synchronized scrolling, a complete column
// schema, windowed cells carrying both raw values and rendered fixed-width text, and a
// mono scope gated by an explicit on/off switch (no hidden auto-on).
// Skips (does not fail) if either the plugin .so or the test module is missing.

use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use plugin_types::{
    AudioFormat, AudioStreamFormat, Cell, ChannelDesc, ColumnDesc, ColumnKind, PlaybackPlugin,
    ReadData, ReadInfo, ReadStatus, RVService, ScrollMode, VizCaps, VizPosition, VizStructure,
};
use services::PluginService;
use std::path::PathBuf;
use vfs::Vfs;

fn so_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_IXALANCE_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-ixalance/build/plugins/ixalance_playback.so")
}

fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_IXALANCE_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/ixalance/test.ixs")
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
fn ixalance_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-ixalance)", path.display());
        return;
    }
    // Canonicalize: the VFS does not resolve `..` components in a path.
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: ixalance test module {} not found (set RV_IXALANCE_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("ixalance-test", Vfs::new());
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

    // Establish playback position.
    render(plugin, user_data, 4);

    // --- structure: pattern cells + scope, synchronized scrolling ---
    let mut st = VizStructure { caps: 0, scroll_mode: ScrollMode::PerChannel, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.get_structure.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() != 0, "ixalance must advertise PatternCells");
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "ixalance must advertise Scope");
    assert!(st.caps & VizCaps::WHOLE_SONG_KNOWN.bits() != 0, "random-access tracker must advertise WholeSongKnown");
    assert_eq!(st.scroll_mode, ScrollMode::Synchronized);
    assert_eq!(st.column_count, 5, "full IT column schema");
    assert!(st.pattern_channel_count > 0, "must report pattern channels");
    assert_eq!(st.scope_channel_count, st.pattern_channel_count);
    let nch = st.pattern_channel_count as usize;

    // --- columns: complete schema, note first, param last ---
    let mut cols = [ColumnDesc { label: [0; 16], char_width: 0, kind: ColumnKind::Custom }; 8];
    let n = (plugin.get_columns.unwrap())(user_data, cols.as_mut_ptr(), cols.len() as u32);
    assert_eq!(n, 5);
    assert_eq!(cols[0].kind, ColumnKind::Note);
    assert_eq!(cols[3].kind, ColumnKind::Effect);
    assert_eq!(cols[4].kind, ColumnKind::Param);

    // --- channels: each named ---
    let mut chans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; nch];
    let nc = (plugin.get_pattern_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc as usize, nch);
    assert_ne!(chans[0].name[0], 0, "channel name should be populated");

    // scope channels are reported too (synchronized tracker mirrors pattern channels)
    let mut schans = vec![ChannelDesc { name: [0; 24], scope_width: 0 }; nch];
    let nsc = (plugin.get_scope_channels.unwrap())(user_data, schans.as_mut_ptr(), schans.len() as u32);
    assert_eq!(nsc as usize, nch, "scope channel count must match structure");
    assert_ne!(schans[0].name[0], 0, "scope channel name should be populated");

    // --- position: window spans the current pattern ---
    let mut pos = VizPosition { order: 0, pattern: 0, row: 0, window_lo: 0, window_hi: 0 };
    assert!((plugin.get_position.unwrap())(user_data, &mut pos));
    assert!(pos.window_hi > 0, "window should span the current pattern's rows");
    let rows = pos.window_hi as usize;

    // get_channel_rows is 0 in Synchronized mode.
    let mut rowbuf = vec![0u32; nch];
    assert_eq!((plugin.get_channel_rows.unwrap())(user_data, rowbuf.as_mut_ptr(), nch as u32), 0);

    // --- cells, all channels: row -> channel -> column, raw + rendered text ---
    let mut cells = vec![Cell { raw: 0, text: [0; 16] }; rows * nch * 5];
    let got = (plugin.get_cells.unwrap())(user_data, -1, 0, rows as u32, cells.as_mut_ptr(), cells.len() as u32);
    assert_eq!(got as usize, rows * nch * 5, "full grid: rows * channels * columns");

    // Every note column (column 0 of each cell triple) must render fixed-width text, and at
    // least one real note must appear somewhere in the pattern carrying BOTH a rendered name
    // and its raw IT note value (proving raw and text are independently populated, not one
    // derived from a zeroed other).
    let mut found_note = false;
    for row in 0..rows {
        for ch in 0..nch {
            let note = &cells[(row * nch + ch) * 5];
            assert_ne!(note.text[0], 0, "note cell must render fixed-width text (never blank)");
            if matches!(note.text[0], b'A'..=b'G') {
                assert!((1..=119).contains(&note.raw), "rendered note must carry a valid raw IT note value, got {}", note.raw);
                found_note = true;
            }
        }
    }
    assert!(found_note, "no rendered note name found in the pattern grid");

    // single channel: row -> column
    let got1 = (plugin.get_cells.unwrap())(user_data, 0, 0, rows as u32, cells.as_mut_ptr(), cells.len() as u32);
    assert_eq!(got1 as usize, rows * 5);

    // --- no hidden auto-on: scope stays silent until enabled ---
    render(plugin, user_data, 4);
    let mut scope = vec![0f32; 1024];
    let ns_off = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(ns_off, 0, "scope must stay silent until set_scope_enabled(true)");

    // --- enable: at least one channel becomes non-silent ---
    (plugin.set_scope_enabled.unwrap())(user_data, true);
    render(plugin, user_data, 40);
    let mut max_peak = 0f32;
    for ch in 0..nch {
        let mut buf = vec![0f32; 1024];
        let n = (plugin.get_scope_samples.unwrap())(user_data, ch as i32, buf.as_mut_ptr(), buf.len() as u32) as usize;
        let peak = buf[..n].iter().fold(0f32, |a, &x| a.max(x.abs()));
        max_peak = max_peak.max(peak);
    }
    assert!(max_peak > 1e-4, "scope is silent after enable (peak {max_peak})");

    // --- disable stops capture; re-enable resumes ---
    (plugin.set_scope_enabled.unwrap())(user_data, false);
    render(plugin, user_data, 5);
    let ns_disabled = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(ns_disabled, 0, "set_scope_enabled(false) must stop capture");

    (plugin.set_scope_enabled.unwrap())(user_data, true);
    render(plugin, user_data, 40);
    let mut resumed = 0u32;
    for ch in 0..nch {
        resumed = (plugin.get_scope_samples.unwrap())(user_data, ch as i32, scope.as_mut_ptr(), scope.len() as u32);
        if resumed > 0 {
            break;
        }
    }
    assert!(resumed > 0, "set_scope_enabled(true) must resume capture");

    (plugin.destroy.unwrap())(user_data);
}
