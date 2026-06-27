// dlopen the real openmpt plugin and drive the value-semantic viz vtable end to end.
// Skips (does not fail) if the plugin .so has not been built with cmake.

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
    if let Ok(p) = std::env::var("RV_OPENMPT_SO") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../playback-openmpt/build/plugins/openmpt_playback.so")
}

// Minimal valid 4-channel ProTracker MOD: one pattern, one square-wave sample,
// a C note retriggered down channel 0 so cells and scope are non-empty.
fn make_mod() -> Vec<u8> {
    let mut m = vec![0u8; 20]; // title
    for i in 0..31 {
        let mut hdr = [0u8; 30];
        if i == 0 {
            let len_words: u16 = 32; // 64 bytes of sample data
            hdr[22] = (len_words >> 8) as u8;
            hdr[23] = (len_words & 0xff) as u8;
            hdr[25] = 64; // volume
            hdr[29] = 1; // repeat length 1 word (non-looping)
        }
        m.extend_from_slice(&hdr);
    }
    m.push(1); // song length: 1 order
    m.push(0x7f); // restart
    m.extend_from_slice(&[0u8; 128]); // order table, all -> pattern 0
    m.extend_from_slice(b"M.K.");

    let mut pat = vec![0u8; 64 * 4 * 4];
    let period: u16 = 428; // a C note
    let sample = 1u8;
    for row in 0..64 {
        if row % 4 == 0 {
            let off = row * 4 * 4; // channel 0
            pat[off] = (sample & 0xf0) | ((period >> 8) as u8 & 0x0f);
            pat[off + 1] = (period & 0xff) as u8;
            pat[off + 2] = (sample & 0x0f) << 4;
        }
    }
    m.extend_from_slice(&pat);

    let mut sd = vec![0u8; 64]; // square wave, signed 8-bit
    for j in 0..64 {
        sd[j] = if j < 32 { 100i8 } else { -100i8 } as u8;
    }
    m.extend_from_slice(&sd);
    m
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
fn openmpt_viz_vtable() {
    let path = so_path();
    if !path.exists() {
        eprintln!("skipping: {} not built (run cmake --build in playback-openmpt)", path.display());
        return;
    }

    let service = PluginService::new("openmpt-test", Vfs::new());
    let svc = service.get_c_api() as *const RVService;

    let lib = unsafe { Library::new(&path) }.expect("dlopen");
    let entry: Symbol<extern "C" fn() -> *const PlaybackPlugin> =
        unsafe { lib.get(b"rv_playback_plugin\0") }.expect("rv_playback_plugin");
    let plugin = unsafe { &*entry() };

    assert_eq!(plugin.api_version, plugin_types::RV_PLAYBACK_PLUGIN_API_VERSION);

    (plugin.static_init.unwrap())(svc);
    let user_data = (plugin.create.unwrap())(svc);
    assert!(!user_data.is_null());

    // Write the module and open it through the plugin's own io path.
    let mut mod_path = std::env::temp_dir();
    mod_path.push("rv_viz_test.mod");
    std::fs::write(&mod_path, make_mod()).unwrap();
    let c_url = CFixedString::from_str(mod_path.to_str().unwrap());
    let rc = (plugin.open.unwrap())(user_data, c_url.as_ptr(), 0, svc);
    assert_eq!(rc, 0, "open failed");

    // Establish playback position.
    render(plugin, user_data, 4);

    // --- structure ---
    let mut st = VizStructure { caps: 0, scroll_mode: ScrollMode::Synchronized, pattern_channel_count: 0, scope_channel_count: 0, column_count: 0 };
    assert!((plugin.get_structure.unwrap())(user_data, &mut st));
    assert!(st.caps & VizCaps::PATTERN_CELLS.bits() != 0);
    assert!(st.caps & VizCaps::SCOPE.bits() != 0);
    assert_eq!(st.scroll_mode, ScrollMode::Synchronized);
    assert_eq!(st.pattern_channel_count, 4);
    assert_eq!(st.column_count, 5);

    // --- columns ---
    let mut cols = [ColumnDesc { label: [0; 16], char_width: 0, kind: ColumnKind::Custom }; 8];
    let n = (plugin.get_columns.unwrap())(user_data, cols.as_mut_ptr(), cols.len() as u32);
    assert_eq!(n, 5);
    assert_eq!(cols[0].kind, ColumnKind::Note);
    assert_eq!(cols[4].kind, ColumnKind::Param);

    // --- channels ---
    let mut chans = [ChannelDesc { name: [0; 24], scope_width: 0 }; 64];
    let nc = (plugin.get_pattern_channels.unwrap())(user_data, chans.as_mut_ptr(), chans.len() as u32);
    assert_eq!(nc, 4);
    assert_ne!(chans[0].name[0], 0, "channel name should be populated");

    // --- position ---
    let mut pos = VizPosition { order: 0, pattern: 0, row: 0, window_lo: 0, window_hi: 0 };
    assert!((plugin.get_position.unwrap())(user_data, &mut pos));
    assert_eq!(pos.window_hi, 64, "window should span the 64-row pattern");

    // get_channel_rows is 0 in Synchronized mode.
    let mut rows = [0u32; 4];
    assert_eq!((plugin.get_channel_rows.unwrap())(user_data, rows.as_mut_ptr(), 4), 0);

    // --- cells, all channels: row -> channel -> column ---
    let mut cells = vec![Cell { raw: 0, text: [0; 16] }; 64 * 4 * 5];
    let got = (plugin.get_cells.unwrap())(user_data, -1, 0, 64, cells.as_mut_ptr(), cells.len() as u32);
    assert_eq!(got, 64 * 4 * 5);
    let note_cell = |row: usize, ch: usize| &cells[(row * 4 + ch) * 5];
    let first = note_cell(0, 0);
    assert_ne!(first.raw, 0, "row0 ch0 note should be set");
    assert!(matches!(first.text[0], b'A'..=b'G'), "note text should render a note name, got {:?}", first.text[0] as char);

    // single channel: row -> column
    let got1 = (plugin.get_cells.unwrap())(user_data, 0, 0, 64, cells.as_mut_ptr(), cells.len() as u32);
    assert_eq!(got1, 64 * 5);

    // --- scope: non-silent on channel 0 ---
    (plugin.set_scope_enabled.unwrap())(user_data, true);
    render(plugin, user_data, 20);
    let mut scope = vec![0f32; 1024];
    let ns = (plugin.get_scope_samples.unwrap())(user_data, 0, scope.as_mut_ptr(), scope.len() as u32);
    assert!(ns > 0, "scope returned no samples");
    let peak = scope[..ns as usize].iter().fold(0f32, |a, &x| a.max(x.abs()));
    assert!(peak > 1e-4, "scope is silent (peak {peak})");

    (plugin.destroy.unwrap())(user_data);
}
