// dlopen the real tfmx plugin and drive the value-semantic viz vtable end to end,
// proving the per-channel non-synchronized scrolling model.
// Skips (does not fail) if either the plugin .so or the test module is missing.

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
    if let Ok(p) = std::env::var("RV_TFMX_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-tfmx/build/plugins/tfmx_playback.so")
}

// Canonical known module: Turrican 2 loader (Chris Hülsbeck), a tiny multi-file TFMX
// (mdat + sibling smpl). Override with RV_TFMX_MODULE to point at any local mdat.* file.
fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_TFMX_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../../../replay_frontend/data/test_data/music/tfmx_v2/\
         ftp.modland.com/pub/modules/TFMX/Chris Huelsbeck/mdat.turrican 2 loading",
    )
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
fn tfmx_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-tfmx)", path.display());
        return;
    }
    // Canonicalize: the VFS does not resolve `..` components in a path.
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: test module {} not found (set RV_TFMX_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("tfmx-test", Vfs::new());
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

    // --- structure: per-channel, non-synchronized ---
    let mut st = VizInfo { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.viz_info.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() != 0);
    assert!(st.caps & VizCaps::SCOPE.bits() != 0);
    assert_eq!(st.scroll_mode, ScrollMode::PerChannel, "tfmx must advertise per-channel scrolling");
    assert!(st.pattern_channel_count > 0, "expected pattern channels");
    assert!(st.scope_channel_count > 0, "expected scope voices");
    assert_eq!(st.column_count, 5);
    let channels = st.pattern_channel_count as usize;

    // --- columns ---
    let mut cols = [ColumnDesc { label: [0; 16], char_width: 0, kind: ColumnKind::Custom }; 8];
    let n = (plugin.tracker_columns.unwrap())(user_data, cols.as_mut_ptr(), cols.len() as u32);
    assert_eq!(n, 5);
    assert_eq!(cols[0].kind, ColumnKind::Note);
    assert_eq!(cols[3].kind, ColumnKind::Effect);
    assert_eq!(cols[4].kind, ColumnKind::Param);

    // --- pattern channels ---
    let mut chans = [ChannelDesc { name: [0; 24], scope_width: 0 }; 64];
    let nc = (plugin.tracker_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc as usize, channels);
    assert_ne!(chans[0].name[0], 0, "channel name should be populated");

    // --- position: window spans the longest track ---
    let mut pos = TrackerPosition { order: 0, pattern: 0, row: 0, window_lo: 0, window_hi: 0 };
    assert!((plugin.tracker_position.unwrap())(user_data, &mut pos));
    assert!(pos.window_hi > 0, "window should span the pattern rows");
    let rows = pos.window_hi as usize;

    // --- per-channel scrolling: one independent row per channel ---
    let mut chrows = vec![0u32; channels];
    let got_rows = (plugin.tracker_channel_rows.unwrap())(user_data, chrows.as_mut_ptr(), channels as u32);
    assert_eq!(got_rows as usize, channels, "per-channel mode reports one row per channel");
    for (i, &r) in chrows.iter().enumerate() {
        assert!(r <= pos.window_hi, "channel {i} row {r} outside window {}", pos.window_hi);
    }
    // Playheads track playback: after rendering further, channels sit at distinct rows.
    // TFMX tracks have different row densities, so non-synchronized scrolling means their
    // current rows genuinely diverge — the core property of the per-channel model (AC #1).
    render(plugin, user_data, 40);
    let mut chrows2 = vec![0u32; channels];
    (plugin.tracker_channel_rows.unwrap())(user_data, chrows2.as_mut_ptr(), channels as u32);
    assert!(chrows2.iter().any(|&r| r > 0), "per-channel playheads did not advance");
    let distinct: std::collections::HashSet<u32> = chrows2.iter().copied().collect();
    assert!(distinct.len() > 1, "expected channels at distinct rows (non-synchronized), got {chrows2:?}");

    // --- cells, all channels: row -> channel -> column (rectangular grid) ---
    let mut cells = vec![PatternCell { raw: 0, text: [0; 16] }; rows * channels * 5];
    let got = (plugin.tracker_cells.unwrap())(user_data, -1, 0, pos.window_hi, cells.as_mut_ptr(), cells.len() as u32);
    assert_eq!(got as usize, rows * channels * 5);

    // single channel returns rows * columns
    let mut one = vec![PatternCell { raw: 0, text: [0; 16] }; rows * 5];
    let got1 = (plugin.tracker_cells.unwrap())(user_data, 0, 0, pos.window_hi, one.as_mut_ptr(), one.len() as u32);
    assert_eq!(got1 as usize, rows * 5);

    // Content: at least one note cell renders a note name, and the effect encoding is
    // standardized — wherever "W" is rendered the raw is the command byte 0xF3, never 'W' (0x57).
    let mut saw_note = false;
    let mut saw_wait = false;
    for r in 0..rows {
        for ch in 0..channels {
            let base = (r * channels + ch) * 5;
            let note = &cells[base];
            if matches!(note.text[0], b'A'..=b'G') {
                saw_note = true;
                // Routing (old dest_channel) is packed into the Note cell's raw high byte;
                // it must be a valid voice index for the host to colour by.
                assert!(
                    (note.raw >> 8) < st.scope_channel_count,
                    "routing {} packed in note raw exceeds voice count {}",
                    note.raw >> 8,
                    st.scope_channel_count
                );
            }
            let eff = &cells[base + 3];
            if eff.text[0] == b'W' {
                assert_eq!(eff.raw, 0xF3, "WAIT effect raw must be the command byte, not the letter 'W'");
                saw_wait = true;
            }
        }
    }
    assert!(saw_note, "expected at least one rendered note name in the pattern");
    assert!(saw_wait, "expected at least one WAIT effect (standardized raw + 'W' text)");

    // --- scope: non-silent on voice 0 ---
    (plugin.scope_enable.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let mut scope = vec![0f32; 1024];
    let ns = (plugin.scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert!(ns > 0, "scope returned no samples");
    let peak = scope[..ns as usize].iter().fold(0f32, |a, &x| a.max(x.abs()));
    assert!(peak > 1e-4, "scope is silent (peak {peak})");

    (plugin.destroy.unwrap())(user_data);
}
