use plugin_types::{
    Cell, ChannelDesc, ColumnDesc, ColumnKind, PlaybackPlugin, ScrollMode, VizCaps, VizPosition,
    VizStructure,
};
use std::os::raw::c_void;

// ponytail: fixed per-channel scope window; widen if a plugin reports more.
const SCOPE_SAMPLE_CAP: usize = 2048;

/// A coherent, frame-stamped copy of a plugin's visualization data for a single
/// instant. Built on the decode thread by value, so it can be handed to the UI
/// thread without the UI ever touching plugin state.
#[derive(Clone, Debug)]
pub struct VizSnapshot {
    /// Cumulative output-frame position this snapshot reflects (host-stamped).
    pub output_frame: u64,
    pub caps: VizCaps,
    pub scroll_mode: ScrollMode,
    pub columns: Vec<ColumnDesc>,
    pub pattern_channels: Vec<ChannelDesc>,
    pub scope_channels: Vec<ChannelDesc>,
    pub position: VizPosition,
    /// Per-channel current row, only in PerChannel scroll mode (else empty).
    pub channel_rows: Vec<u32>,
    /// Window `position.window_lo..window_hi`, row-major across pattern channels
    /// then columns (the `channel = -1` layout of `get_cells`).
    pub cells: Vec<Cell>,
    /// Per scope-channel waveform samples.
    pub scope: Vec<Vec<f32>>,
    /// Per pattern-channel VU level, only when the VU cap is advertised.
    pub vu: Vec<f32>,
}

/// Allocate `count` slots, let the getter fill them, truncate to what it wrote.
fn read_vec<T: Copy>(count: u32, zero: T, fill: impl FnOnce(*mut T, u32) -> u32) -> Vec<T> {
    let mut v = vec![zero; count as usize];
    let n = fill(v.as_mut_ptr(), count);
    v.truncate(n as usize);
    v
}

/// Call the plugin's viz getters and copy everything into a value-semantic
/// snapshot. Must run on the decode thread (same thread as `read_data`).
/// Returns `None` when the plugin exposes no structure (no viz capabilities).
pub fn build_snapshot(
    plugin: &PlaybackPlugin,
    user_data: *mut c_void,
    output_frame: u64,
) -> Option<VizSnapshot> {
    let get_structure = plugin.get_structure?;
    let mut st = VizStructure {
        caps: 0,
        scroll_mode: ScrollMode::Synchronized,
        pattern_channel_count: 0,
        scope_channel_count: 0,
        column_count: 0,
    };
    if !get_structure(user_data, &mut st) {
        return None;
    }
    let caps = VizCaps::from_bits_truncate(st.caps);

    let columns = read_vec(
        st.column_count,
        ColumnDesc { label: [0; 16], char_width: 0, kind: ColumnKind::Note },
        |ptr, cap| plugin.get_columns.map_or(0, |f| f(user_data, ptr, cap)),
    );

    let chan_zero = ChannelDesc { name: [0; 24], scope_width: 0 };
    let pattern_channels = read_vec(st.pattern_channel_count, chan_zero, |ptr, cap| {
        plugin.get_pattern_channels.map_or(0, |f| f(user_data, ptr, cap))
    });
    let scope_channels = read_vec(st.scope_channel_count, chan_zero, |ptr, cap| {
        plugin.get_scope_channels.map_or(0, |f| f(user_data, ptr, cap))
    });

    // A zeroed position means "not yet known"; skip cells rather than emit a
    // bogus row-0 window the consumer can't tell apart from real position 0.
    let mut position = VizPosition { order: 0, pattern: 0, row: 0, window_lo: 0, window_hi: 0 };
    let have_position = plugin.get_position.map_or(false, |f| f(user_data, &mut position));

    let channel_rows = if st.scroll_mode == ScrollMode::PerChannel {
        read_vec(st.pattern_channel_count, 0u32, |ptr, cap| {
            plugin.get_channel_rows.map_or(0, |f| f(user_data, ptr, cap))
        })
    } else {
        Vec::new()
    };

    let cells = if have_position {
        let rows = position.window_hi.saturating_sub(position.window_lo);
        let cell_cap = rows
            .saturating_mul(st.pattern_channel_count)
            .saturating_mul(st.column_count);
        read_vec(cell_cap, Cell { raw: 0, text: [0; 16] }, |ptr, cap| {
            plugin
                .get_cells
                .map_or(0, |f| f(user_data, -1, position.window_lo, position.window_hi, ptr, cap))
        })
    } else {
        Vec::new()
    };

    let mut scope = Vec::new();
    if caps.contains(VizCaps::SCOPE) {
        if let Some(f) = plugin.get_scope_samples {
            for ch in 0..st.scope_channel_count {
                let mut buf = vec![0f32; SCOPE_SAMPLE_CAP];
                let n = f(user_data, ch as i32, buf.as_mut_ptr(), SCOPE_SAMPLE_CAP as u32);
                buf.truncate(n as usize);
                scope.push(buf);
            }
        }
    }

    let vu = if caps.contains(VizCaps::VU) {
        read_vec(st.pattern_channel_count, 0f32, |ptr, cap| {
            plugin.get_vu.map_or(0, |f| f(user_data, ptr, cap))
        })
    } else {
        Vec::new()
    };

    Some(VizSnapshot {
        output_frame,
        caps,
        scroll_mode: st.scroll_mode,
        columns,
        pattern_channels,
        scope_channels,
        position,
        channel_rows,
        cells,
        scope,
        vu,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A hand-written stub vtable with known viz outputs. user_data is unused.
    extern "C" fn get_structure(_ud: *mut c_void, out: *mut VizStructure) -> bool {
        unsafe {
            *out = VizStructure {
                caps: (VizCaps::PATTERN_CELLS | VizCaps::SCOPE | VizCaps::VU).bits(),
                scroll_mode: ScrollMode::Synchronized,
                pattern_channel_count: 2,
                scope_channel_count: 2,
                column_count: 3,
            };
        }
        true
    }

    extern "C" fn get_columns(_ud: *mut c_void, out: *mut ColumnDesc, cap: u32) -> u32 {
        let kinds = [ColumnKind::Note, ColumnKind::Volume, ColumnKind::Effect];
        let n = kinds.len().min(cap as usize);
        for (i, &kind) in kinds.iter().take(n).enumerate() {
            unsafe { *out.add(i) = ColumnDesc { label: [0; 16], char_width: 3, kind } };
        }
        n as u32
    }

    fn named(s: &str) -> ChannelDesc {
        let mut name = [0u8; 24];
        let b = s.as_bytes();
        name[..b.len()].copy_from_slice(b);
        ChannelDesc { name, scope_width: 1 }
    }

    extern "C" fn get_pattern_channels(_ud: *mut c_void, out: *mut ChannelDesc, cap: u32) -> u32 {
        let chans = [named("left"), named("right")];
        let n = chans.len().min(cap as usize);
        for (i, &c) in chans.iter().take(n).enumerate() {
            unsafe { *out.add(i) = c };
        }
        n as u32
    }

    extern "C" fn get_position(_ud: *mut c_void, out: *mut VizPosition) -> bool {
        unsafe {
            *out = VizPosition { order: 1, pattern: 2, row: 4, window_lo: 0, window_hi: 8 };
        }
        true
    }

    extern "C" fn get_cells(
        _ud: *mut c_void,
        _channel: i32,
        row_lo: u32,
        row_hi: u32,
        out: *mut Cell,
        cap: u32,
    ) -> u32 {
        // channel == -1: all channels, row -> channel -> column (2 chans, 3 cols).
        let total = (row_hi - row_lo) as usize * 2 * 3;
        let n = total.min(cap as usize);
        for i in 0..n {
            unsafe { *out.add(i) = Cell { raw: i as u32, text: [0; 16] } };
        }
        n as u32
    }

    extern "C" fn get_scope_samples(_ud: *mut c_void, channel: i32, out: *mut f32, cap: u32) -> u32 {
        let n = 64.min(cap as usize);
        for i in 0..n {
            // channel 0 is a non-silent ramp, channel 1 is constant.
            let v = if channel == 0 { i as f32 / 64.0 } else { 0.5 };
            unsafe { *out.add(i) = v };
        }
        n as u32
    }

    extern "C" fn get_vu(_ud: *mut c_void, out: *mut f32, cap: u32) -> u32 {
        let vals = [0.8f32, 0.3];
        let n = vals.len().min(cap as usize);
        for (i, &v) in vals.iter().take(n).enumerate() {
            unsafe { *out.add(i) = v };
        }
        n as u32
    }

    fn stub_plugin() -> PlaybackPlugin {
        // Zeroed => all Option<fn> are None and pointers null; fill only the viz slots.
        let mut p: PlaybackPlugin = unsafe { std::mem::zeroed() };
        p.get_structure = Some(get_structure);
        p.get_columns = Some(get_columns);
        p.get_pattern_channels = Some(get_pattern_channels);
        p.get_scope_channels = Some(get_pattern_channels);
        p.get_position = Some(get_position);
        p.get_cells = Some(get_cells);
        p.get_scope_samples = Some(get_scope_samples);
        p.get_vu = Some(get_vu);
        p
    }

    // Raw pointers make PlaybackPlugin !Send; the snapshot itself is value-semantic
    // and Send, which is the whole point of the seam.
    struct SendPlugin(PlaybackPlugin);
    unsafe impl Send for SendPlugin {}

    #[test]
    fn snapshot_matches_stub_across_thread() {
        let plugin = SendPlugin(stub_plugin());
        let (tx, rx) = std::sync::mpsc::channel();

        // Build on a "decode" thread; the plugin never leaves it.
        let producer = std::thread::spawn(move || {
            let snap = build_snapshot(&plugin.0, std::ptr::null_mut(), 12_345)
                .expect("stub advertises structure");
            tx.send(snap).unwrap();
        });

        // Consumer thread only ever touches the value snapshot.
        let snap = rx.recv().unwrap();
        producer.join().unwrap();

        assert_eq!(snap.output_frame, 12_345);
        assert_eq!(snap.caps, VizCaps::PATTERN_CELLS | VizCaps::SCOPE | VizCaps::VU);
        assert_eq!(snap.scroll_mode, ScrollMode::Synchronized);

        assert_eq!(snap.columns.len(), 3);
        assert_eq!(snap.columns[0].kind, ColumnKind::Note);
        assert_eq!(snap.columns[2].kind, ColumnKind::Effect);

        assert_eq!(snap.pattern_channels.len(), 2);
        assert_eq!(&snap.pattern_channels[0].name[..4], b"left");
        assert_eq!(snap.scope_channels.len(), 2);

        assert_eq!(snap.position.row, 4);
        assert_eq!(snap.position.window_hi, 8);
        // Synchronized mode => no per-channel rows.
        assert!(snap.channel_rows.is_empty());

        // window 8 rows * 2 channels * 3 columns, in order.
        assert_eq!(snap.cells.len(), 8 * 2 * 3);
        assert_eq!(snap.cells[0].raw, 0);
        assert_eq!(snap.cells[47].raw, 47);

        assert_eq!(snap.scope.len(), 2);
        assert_eq!(snap.scope[0].len(), 64);
        let peak = snap.scope[0].iter().fold(0f32, |a, &x| a.max(x.abs()));
        assert!(peak > 0.0, "scope channel 0 should be non-silent");

        assert_eq!(snap.vu, vec![0.8, 0.3]);
    }

    // PerChannel scroll mode with only the PATTERN_CELLS cap: channel_rows is
    // populated, and the scope/vu cap-absent guards keep those vectors empty.
    extern "C" fn get_structure_per_channel(_ud: *mut c_void, out: *mut VizStructure) -> bool {
        unsafe {
            *out = VizStructure {
                caps: VizCaps::PATTERN_CELLS.bits(),
                scroll_mode: ScrollMode::PerChannel,
                pattern_channel_count: 3,
                scope_channel_count: 0,
                column_count: 1,
            };
        }
        true
    }

    extern "C" fn get_channel_rows(_ud: *mut c_void, out: *mut u32, cap: u32) -> u32 {
        let rows = [10u32, 20, 30];
        let n = rows.len().min(cap as usize);
        for (i, &r) in rows.iter().take(n).enumerate() {
            unsafe { *out.add(i) = r };
        }
        n as u32
    }

    #[test]
    fn snapshot_per_channel_and_cap_gating() {
        let mut p: PlaybackPlugin = unsafe { std::mem::zeroed() };
        p.get_structure = Some(get_structure_per_channel);
        p.get_position = Some(get_position);
        p.get_channel_rows = Some(get_channel_rows);
        // No scope/vu getters and the caps bits are clear; those vecs stay empty.

        let snap = build_snapshot(&p, std::ptr::null_mut(), 1).unwrap();
        assert_eq!(snap.scroll_mode, ScrollMode::PerChannel);
        assert_eq!(snap.channel_rows, vec![10, 20, 30]);
        assert!(snap.scope.is_empty());
        assert!(snap.vu.is_empty());
    }

    #[test]
    fn no_structure_yields_no_snapshot() {
        let p: PlaybackPlugin = unsafe { std::mem::zeroed() };
        // get_structure is None -> the plugin advertises no viz surface.
        assert!(build_snapshot(&p, std::ptr::null_mut(), 0).is_none());
    }

    #[test]
    fn unknown_position_yields_no_cells() {
        let mut p: PlaybackPlugin = unsafe { std::mem::zeroed() };
        p.get_structure = Some(get_structure);
        p.get_cells = Some(get_cells);
        // No get_position -> have_position is false -> cells stay empty.
        let snap = build_snapshot(&p, std::ptr::null_mut(), 0).unwrap();
        assert!(snap.cells.is_empty());
    }
}
