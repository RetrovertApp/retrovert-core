// dlopen the real hively plugin and drive the value-semantic viz vtable end to end
// against a known AHX module. Skips (does not fail) if the plugin .so has not been
// built or the test module is missing.

use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use plugin_types::{
    AudioFormat, AudioStreamFormat, PatternCell, ChannelDesc, ColumnDesc, ColumnKind, PlaybackPlugin,
    ReadData, ReadInfo, ReadStatus, RVService, ScrollMode, VizCaps, TrackerPosition, VizInfo,
};
use services::PluginService;
use std::path::PathBuf;
use vfs::Vfs;

fn so_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_HIVELY_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-hively/build/plugins/hively_playback.so")
}

fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_HIVELY_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/hively/2_much_pressure.ahx")
}

fn render(plugin: &PlaybackPlugin, user_data: *mut std::ffi::c_void, reads: usize) {
    let mut buf = vec![0f32; 1024 * 2];
    for _ in 0..reads {
        let rd = ReadData {
            channels_output: buf.as_mut_ptr() as _,
            channels_output_max_bytes_size: (buf.len() * 4) as u32,
            info: ReadInfo {
                format: AudioFormat { audio_format: AudioStreamFormat::F32, channel_count: 2, sample_rate: 48000 },
                frame_count: 0,
                status: ReadStatus::DecodingRequest,
            },
        };
        (plugin.read_data.unwrap())(user_data, rd);
    }
}

#[test]
fn hively_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-hively)", path.display());
        return;
    }
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: hively test module {} not found (set RV_HIVELY_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("hively-test", Vfs::new());
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
    assert_eq!(rc, 0, "open failed");

    render(plugin, user_data, 4);

    // --- structure: synchronized pattern cells + scope, whole song known ---
    let mut st = VizInfo { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.viz_info.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() != 0, "PatternCells cap missing");
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "Scope cap missing");
    assert!(st.caps & VizCaps::WHOLE_SONG_KNOWN.bits() != 0, "WholeSongKnown cap missing");
    assert_eq!(st.scroll_mode, ScrollMode::Synchronized);
    assert!(st.pattern_channel_count > 0, "no pattern channels");
    assert_eq!(st.scope_channel_count, st.pattern_channel_count);
    assert_eq!(st.column_count, 6);
    let chans = st.pattern_channel_count as usize;

    // --- columns ---
    let mut cols = [ColumnDesc { label: [0; 16], char_width: 0, kind: ColumnKind::Custom }; 8];
    let n = (plugin.tracker_columns.unwrap())(user_data, cols.as_mut_ptr(), cols.len() as u32);
    assert_eq!(n, 6);
    let want_kinds = [
        ColumnKind::Note, ColumnKind::Instrument, ColumnKind::Effect,
        ColumnKind::Param, ColumnKind::Effect, ColumnKind::Param,
    ];
    for (i, k) in want_kinds.iter().enumerate() {
        assert_eq!(cols[i].kind, *k, "column {i} kind mismatch");
        assert_ne!(cols[i].char_width, 0, "column {i} width unset");
        assert_ne!(cols[i].label[0], 0, "column {i} label empty");
    }

    // --- channels ---
    let mut ch = [ChannelDesc { name: [0; 24], scope_width: 0 }; 64];
    let nc = (plugin.tracker_channels.unwrap())(user_data, ch.as_mut_ptr(), ch.len() as u32);
    assert_eq!(nc as usize, chans);
    assert_ne!(ch[0].name[0], 0, "channel name should be populated");
    let mut scope_ch = [ChannelDesc { name: [0; 24], scope_width: 0 }; 64];
    let nsc = (plugin.scope_channels.unwrap())(user_data, scope_ch.as_mut_ptr(), scope_ch.len() as u32);
    assert_eq!(nsc as usize, chans);
    assert_eq!(scope_ch[0].scope_width, 1, "hively scope is mono");

    // --- position: window spans the track length ---
    let mut pos = TrackerPosition { order: 0, pattern: 0, row: 0, window_lo: 0, window_hi: 0 };
    assert!((plugin.tracker_position.unwrap())(user_data, &mut pos));
    assert!(pos.window_hi > pos.window_lo, "window should be non-empty");
    let rows = pos.window_hi as usize;

    assert_eq!((plugin.tracker_channel_rows.unwrap())(user_data, std::ptr::null_mut(), 0), 0);

    // --- cells, all channels: row -> channel -> column ---
    let total = rows * chans * 6;
    let mut all = vec![PatternCell { raw: 0, text: [0; 16] }; total];
    let got = (plugin.tracker_cells.unwrap())(user_data, -1, 0, pos.window_hi, all.as_mut_ptr(), all.len() as u32);
    assert_eq!(got as usize, total, "all-channel cell count mismatch");

    // Content checks: at least one note cell renders an A-G note name (raw + text),
    // and every non-Note column (Inst / FX / Prm / FX2 / Pr2) with a non-zero raw
    // renders that raw as fixed-width hex — proving the effect encoding keeps the raw
    // command byte and derives the text from it, never the reverse.
    let mut found_note = false;
    let mut found_encoded = false;
    for r in 0..rows {
        for c in 0..chans {
            let base = (r * chans + c) * 6;
            let note = &all[base];
            if note.raw != 0 {
                assert!(matches!(note.text[0], b'A'..=b'G'), "note text should render a note name, got {:?}", note.text[0] as char);
                found_note = true;
            }
            for col in 1..6 {
                let cell = &all[base + col];
                if cell.raw != 0 {
                    assert!(cell.text[0].is_ascii_hexdigit(), "col {col} raw {:#x} should render as hex, got {:?}", cell.raw, cell.text[0] as char);
                    found_encoded = true;
                }
            }
        }
    }
    assert!(found_note, "expected at least one note cell in the pattern");
    assert!(found_encoded, "expected at least one encoded inst/effect/param cell");

    // single channel: row -> column, and content matches the channel-0 slice of the
    // all-channel fill (same raw per row/column) — proves the single-channel path
    // returns the right rows, not zeros or a wrong slice.
    let mut one = vec![PatternCell { raw: 0, text: [0; 16] }; rows * 6];
    let got1 = (plugin.tracker_cells.unwrap())(user_data, 0, 0, pos.window_hi, one.as_mut_ptr(), one.len() as u32);
    assert_eq!(got1 as usize, rows * 6);
    for r in 0..rows {
        for col in 0..6 {
            assert_eq!(one[r * 6 + col].raw, all[(r * chans + 0) * 6 + col].raw, "single-channel cell ({r},{col}) raw mismatch");
        }
    }

    // --- scope: silent until enabled, non-silent while enabled, silent again after disable ---
    let mut scope = vec![0f32; 1024];
    let off = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(off, 0, "scope must stay silent until scope_enable(true)");

    (plugin.scope_enable.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let ns = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert!(ns > 0, "scope returned no samples");
    let peak = scope[..ns as usize].iter().fold(0f32, |a, &x| a.max(x.abs()));
    assert!(peak > 1e-4, "scope is silent (peak {peak})");

    (plugin.scope_enable.unwrap())(user_data, false);
    let off2 = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert_eq!(off2, 0, "scope must report nothing once disabled again");

    (plugin.destroy.unwrap())(user_data);
}
