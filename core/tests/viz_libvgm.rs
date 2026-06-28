// dlopen the real libvgm plugin and drive the value-semantic viz vtable end to end.
// libvgm parses the whole VGM register stream at open, so it proves the
// per-channel-scrolling + WholeSongKnown model: a window spanning every row served
// through the standard windowed tracker_cells (no private native_pattern_data pointer).
// Skips (does not fail) if the plugin .so or the test module is missing, and adapts
// to a .so built without HAS_VGM_PATTERN (scope-only) so both build configs pass.

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
    if let Ok(p) = std::env::var("RV_LIBVGM_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-libvgm/build/plugins/libvgm_playback.so")
}

// Canonical known module: a YM2612 VGM (Phantasy Star) — has pattern cells and per-channel
// scope capture. Override with RV_LIBVGM_MODULE to point at any local VGM/VGZ.
fn module_path() -> PathBuf {
    if let Ok(p) = std::env::var("RV_LIBVGM_MODULE") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../replay_frontend/data/test_data/music/vgm/01 - Phantasy.vgm")
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

// VGM_EFFECT_VOLUME from the plugin's vgm_quantize.h: the standardized effect command
// stored in the Eff cell's raw, with "V" only in the rendered text.
const VGM_EFFECT_VOLUME: u32 = 1;

#[test]
fn libvgm_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-libvgm)", path.display());
        return;
    }
    // Canonicalize: the VFS does not resolve `..` components in a path.
    let module = match std::fs::canonicalize(module_path()) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping: test module {} not found (set RV_LIBVGM_MODULE)", module_path().display());
            return;
        }
    };

    let service = PluginService::new("libvgm-test", Vfs::new());
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

    // Establish playback position and let scope capture warm up.
    (plugin.scope_enable.unwrap())(user_data, true);
    render(plugin, user_data, 8);

    // --- structure ---
    let mut st = VizInfo { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.viz_info.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::SCOPE.bits() != 0, "libvgm should advertise scope");
    assert!(st.scope_channel_count > 0, "expected scope voices");

    let has_pattern = st.caps & VizCaps::PATTERN_CELLS.bits() != 0;
    if has_pattern {
        // libvgm knows the whole register stream at open.
        assert!(st.caps & VizCaps::WHOLE_SONG_KNOWN.bits() != 0, "pattern build must advertise WholeSongKnown");
        assert_eq!(st.scroll_mode, ScrollMode::PerChannel, "libvgm scrolls per-channel");
        assert!(st.pattern_channel_count > 0, "expected pattern channels");
        assert_eq!(st.column_count, 4);
        let channels = st.pattern_channel_count as usize;

        // --- columns ---
        let mut cols = [ColumnDesc { label: [0; 16], char_width: 0, kind: ColumnKind::Custom }; 8];
        let n = (plugin.tracker_columns.unwrap())(user_data, cols.as_mut_ptr(), cols.len() as u32);
        assert_eq!(n, 4);
        assert_eq!(cols[0].kind, ColumnKind::Note);
        assert_eq!(cols[1].kind, ColumnKind::Volume);
        assert_eq!(cols[2].kind, ColumnKind::Effect);
        assert_eq!(cols[3].kind, ColumnKind::Param);

        // --- pattern channels ---
        let mut chans = [ChannelDesc { name: [0; 24], scope_width: 0 }; 64];
        let nc = (plugin.tracker_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
        assert_eq!(nc as usize, channels);
        assert_ne!(chans[0].name[0], 0, "channel name should be populated");

        // --- position: window spans the longest channel ---
        let mut pos = TrackerPosition { order: 0, pattern: 0, row: 0, window_lo: 0, window_hi: 0 };
        assert!((plugin.tracker_position.unwrap())(user_data, &mut pos));
        assert!(pos.window_hi > 0, "window should span the pattern rows");
        let rows = pos.window_hi as usize;

        // --- per-channel scrolling: one independent row per channel, all inside the window ---
        let mut chrows = vec![0u32; channels];
        let got_rows = (plugin.tracker_channel_rows.unwrap())(user_data, chrows.as_mut_ptr(), channels as u32);
        assert_eq!(got_rows as usize, channels, "per-channel mode reports one row per channel");
        for (i, &r) in chrows.iter().enumerate() {
            assert!(r <= pos.window_hi, "channel {i} row {r} outside window {}", pos.window_hi);
        }
        // Playheads track playback: after rendering further at least one channel's row advances —
        // proves tracker_channel_rows is wired to the live position, not a zero stub.
        render(plugin, user_data, 40);
        let mut chrows2 = vec![0u32; channels];
        (plugin.tracker_channel_rows.unwrap())(user_data, chrows2.as_mut_ptr(), channels as u32);
        assert!(
            chrows2.iter().zip(&chrows).any(|(&b, &a)| b > a),
            "per-channel playheads did not advance: {chrows:?} -> {chrows2:?}"
        );

        // --- cells, all channels: row -> channel -> column (rectangular grid) ---
        let mut cells = vec![PatternCell { raw: 0, text: [0; 16] }; rows * channels * 4];
        let got = (plugin.tracker_cells.unwrap())(user_data, -1, 0, pos.window_hi, cells.as_mut_ptr(), cells.len() as u32);
        assert_eq!(got as usize, rows * channels * 4);

        // single channel returns rows * columns
        let mut one = vec![PatternCell { raw: 0, text: [0; 16] }; rows * 4];
        let got1 = (plugin.tracker_cells.unwrap())(user_data, 0, 0, pos.window_hi, one.as_mut_ptr(), one.len() as u32);
        assert_eq!(got1 as usize, rows * 4);

        // Content: at least one rendered note name, and the effect encoding is standardized —
        // wherever "V" is rendered the raw is the command id VGM_EFFECT_VOLUME, never 'V' (0x56).
        let mut saw_note = false;
        for r in 0..rows {
            for ch in 0..channels {
                let base = (r * channels + ch) * 4;
                let note = &cells[base];
                if matches!(note.text[0], b'A'..=b'G') {
                    saw_note = true;
                    assert_eq!(note.raw, note.raw & 0x7f, "MIDI note raw should fit 0..127");
                }
                let eff = &cells[base + 2];
                if eff.text[0] == b'V' {
                    assert_eq!(eff.raw, VGM_EFFECT_VOLUME, "effect raw must be the command id, not the letter 'V'");
                }
            }
        }
        assert!(saw_note, "expected at least one rendered note name in the pattern");
    } else {
        // Built without HAS_VGM_PATTERN: scope-only, no pattern surface.
        eprintln!("note: libvgm .so built without HAS_VGM_PATTERN — verifying scope-only path");
        assert_eq!(st.pattern_channel_count, 0);
        assert_eq!(st.column_count, 0);
        let mut cells = [PatternCell { raw: 0, text: [0; 16] }; 16];
        assert_eq!((plugin.tracker_cells.unwrap())(user_data, -1, 0, 64, cells.as_mut_ptr(), cells.len() as u32), 0);
    }

    // --- scope: non-silent on at least one voice (real chip emulation) ---
    render(plugin, user_data, 24);
    let mut scope = vec![0f32; 1024];
    let mut peak = 0f32;
    for ch in 0..st.scope_channel_count as i32 {
        let ns = (plugin.scope_samples.unwrap())(user_data, ch, scope.as_mut_ptr(), scope.len() as u32);
        peak = scope[..ns as usize].iter().fold(peak, |a, &x| a.max(x.abs()));
    }
    assert!(peak > 1e-4, "scope is silent across all voices (peak {peak})");

    (plugin.destroy.unwrap())(user_data);
}
