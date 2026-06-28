use plugin_types::{
    PlaybackPlugin, 
    ResamplePlugin,
    AudioFormat, 
    AudioStreamFormat, 
    ReadData, 
    ReadInfo, 
    ReadStatus, ConvertConfig
};
use crossbeam_channel::{Sender, Receiver, unbounded};
use log::{error, trace};
use anyhow::{Result, bail};
use parking_lot::Mutex;
use std::{
    thread,
    os::raw::c_void,
    sync::Arc,
};

use crate::plugin_handler::ResamplePlugins;
use crate::visualization::{build_snapshot, VizSnapshot};

/// Latest visualization snapshot, published by the decode thread and read by the
/// UI thread. `None` until the active plugin produces its first frame.
pub type VizSnapshotSlot = Arc<Mutex<Option<VizSnapshot>>>;

// Temp buffer size is 1 sec of audio data for 2 channels floats
const TEMP_BUFFER_SIZE: usize = 48000 * 4 * 2;
const DEFAULT_AUDIO_FORMAT: AudioFormat = AudioFormat {
    audio_format: AudioStreamFormat::F32,
    channel_count: 2,
    sample_rate: 48000,
};

unsafe impl Sync for PlaybackPluginInstance {}
unsafe impl Send for PlaybackPluginInstance {}
unsafe impl Sync for ResamplePluginInstance {}
unsafe impl Send for ResamplePluginInstance {}

#[derive(Clone, Debug)]
pub struct Playback {
    pub channel: Sender<PlaybackMessage>,
    /// Shared with the decode thread; the UI reads the latest snapshot from here.
    pub viz_snapshot: VizSnapshotSlot,
}

pub struct PlaybackHandle {
    pub channel: Receiver<PlaybackReply>,
}

impl Playback {
    /// Loads the first file given a url. If an known archive is encounterd the first file will be extracted
    /// (given if it has a file listing) and that will be returned until a file is encountered. If there are
    /// no files an error will/archive will be returned instead and the user code has to handle it
    pub fn queue_playback(&self, playback_instance: PlaybackPluginInstance) -> Result<PlaybackHandle> {
        let (thread_send, main_recv) = unbounded::<PlaybackReply>();

        self.channel.send(PlaybackMessage::QueuePlayback(playback_instance, thread_send))?;

        Ok(PlaybackHandle { channel: main_recv })
    }
}

#[derive(Clone)]
pub struct PlaybackPluginInstance {
    pub user_data: *mut c_void,
    pub plugin: PlaybackPlugin,
}

#[derive(Clone, Debug)]
pub struct ResamplePluginInstance {
    pub user_data: *mut c_void,
    pub plugin: ResamplePlugin,
}

#[derive(Default, PartialEq, PartialOrd)]
pub struct Index {
    pub value: u64,
}

impl Index {
    #[inline(always)]
    pub fn bump_generation(&mut self) {
        self.value = self.value.wrapping_add(1 << 32u64);
    }

    #[inline(always)]
    pub fn add(&mut self, v: usize) {
        self.value += v as u64;
    }

    #[inline(always)]
    pub fn set(&mut self, v: usize) {
        self.value = (self.value & 0xffff_ffff_0000_0000) | (v as u64);
    }

    #[inline(always)]
    pub fn get(&mut self) -> usize {
        (self.value & 0x0000_0000_ffff_ffff) as usize
    }
}


pub struct PlaybackInternal {
    pub players: Vec<(PlaybackPluginInstance, Sender<PlaybackReply>)>,
    /// List of all resample plugins
    pub resample_plugins: ResamplePlugins,
    /// Used for generating data to requests
    pub output_resampler: ResamplePluginInstance,
    /// Resampler when reading data from plugins 
    pub plugin_resampler: ResamplePluginInstance, 
    /// Temporary buffer used when requesting data from a player plugin
    temp_gen: [Vec<u8>; 2],
    /// Ring buffer for audio output
    ring_buffer: Vec<u8>,
    read_index: Index,
    write_index: Index,
    // Format used for the internal ring-buffer
    internal_format: AudioFormat,
    // Format used for the current playing plugin
    plugin_format: AudioFormat,
    /// Format of the most recent output request; the output resampler is only
    /// reconfigured when a request format differs from this.
    last_request_format: AudioFormat,
    /// Cumulative frames decoded for the active plugin; stamped onto each snapshot.
    output_frame: u64,
    /// Latest viz snapshot, shared with the UI thread.
    viz_snapshot: VizSnapshotSlot,
}

pub enum PlaybackMessage {
    QueuePlayback(PlaybackPluginInstance, Sender<PlaybackReply>),
    GetData(plugin_types::AudioFormat, usize, Sender<PlaybackReply>),
    GetTrackerPosition(Sender<PlaybackReply>)
}

pub enum PlaybackReply {
    /// Playback of the request has ended
    PlaybackEnded,
    /// No data is available yet for the request
    NoData,
    /// Current tracker position for the active player
    TrackerPosition(u64),
    /// Returns generated audio data back to the requester
    Data(Box<[u8]>),
}

impl PlaybackInternal {
    fn new(resample_plugins: ResamplePlugins, viz_snapshot: VizSnapshotSlot) -> Result<PlaybackInternal> {
        let output_resampler = Self::create_default_resample_plugin(&resample_plugins)?;
        let plugin_resampler = Self::create_default_resample_plugin(&resample_plugins)?;
        Ok(Self::with_resamplers(resample_plugins, output_resampler, plugin_resampler, viz_snapshot))
    }

    fn with_resamplers(
        resample_plugins: ResamplePlugins,
        output_resampler: ResamplePluginInstance,
        plugin_resampler: ResamplePluginInstance,
        viz_snapshot: VizSnapshotSlot,
    ) -> PlaybackInternal {
        let ring_buffer_size = get_byte_size_format(DEFAULT_AUDIO_FORMAT, DEFAULT_AUDIO_FORMAT.sample_rate as usize * 2);
        PlaybackInternal {
            // 2 sec of buffering for now
            ring_buffer: vec![0u8; ring_buffer_size],
            temp_gen: [vec![0u8; TEMP_BUFFER_SIZE], vec![0u8; TEMP_BUFFER_SIZE]],
            players: Vec::new(),
            resample_plugins,
            output_resampler,
            plugin_resampler,
            read_index: Index::default(),
            write_index: Index::default(),
            internal_format: DEFAULT_AUDIO_FORMAT,
            last_request_format: DEFAULT_AUDIO_FORMAT,
            plugin_format: DEFAULT_AUDIO_FORMAT,
            output_frame: 0,
            viz_snapshot,
        }
    }

    fn create_default_resample_plugin(resample_plugins: &ResamplePlugins) -> Result<ResamplePluginInstance> {
        let resample_plugins = resample_plugins.read();

        if resample_plugins.is_empty() {
            bail!("No resample plugin(s) found. Unable to setup Retrovert playback");
        }
        
        let op = &resample_plugins[0];

        let plugin_name = op.plugin_funcs.get_name();
        let service_funcs = op.service.get_c_api();
        let user_data = unsafe {
            (op.plugin_funcs.create.unwrap())(service_funcs as *const _)
        };

        if user_data.is_null() {
            bail!("{} : unable to allocate instance", plugin_name);
        }

        let config = plugin_types::ConvertConfig { input: DEFAULT_AUDIO_FORMAT, output: DEFAULT_AUDIO_FORMAT };

        unsafe { (op.plugin_funcs.set_config.unwrap())(user_data, &config); }
        
        trace!("Created default resample plugin: {}", plugin_name);

        Ok(ResamplePluginInstance { user_data, plugin: op.plugin_funcs })
    }
}

// Send back data to the requester if possible
fn get_data(state: &mut PlaybackInternal, format: AudioFormat, frames: usize, msg: &Sender<PlaybackReply>) {
    // Update format if it differs
    if state.last_request_format != format {
        let config = ConvertConfig { input: DEFAULT_AUDIO_FORMAT, output: format };
        unsafe { (state.output_resampler.plugin.set_config.unwrap())(state.output_resampler.user_data, &config) };
        state.last_request_format = format;
    }

    // A dropped reply receiver just means the requester gave up; ignore the
    // send error rather than panicking the decode thread.
    if state.read_index >= state.write_index {
        let _ = msg.send(PlaybackReply::NoData);
        return;
    }

    let output_bytes_size = get_byte_size_format(format, frames);
    let ring_buffer_len = state.ring_buffer.len();

    // if we haven't generated any data yet
    if output_bytes_size as u64 > state.write_index.value {
        let _ = msg.send(PlaybackReply::NoData);
        return;
    }

    let mut dest = vec![0u8; output_bytes_size].into_boxed_slice();

    let read_index = state.read_index.get();

    // if format differs from the default format we need to convert it
    if format != DEFAULT_AUDIO_FORMAT {
        let required_input_frames = unsafe {
            (state.output_resampler.plugin.get_required_input_frame_count.unwrap())(state.output_resampler.user_data, frames as _)
        };

        let bytes_size = get_byte_size_format(DEFAULT_AUDIO_FORMAT, required_input_frames as _);

        // if read are within the ring-buffer range we can just convert directly from it to the output
        if (read_index + bytes_size) < ring_buffer_len {
            unsafe {
                (state.output_resampler.plugin.convert.unwrap())(state.output_resampler.user_data,
                    dest.as_mut_ptr() as _, 
                    state.ring_buffer[read_index..].as_mut_ptr() as _, 
                    required_input_frames as _);
            }
            state.read_index.add(bytes_size);
        } else {
            let rem_count = state.ring_buffer[read_index..].len();
            let rest_count = bytes_size - rem_count; 
            // if we need to wrap the ring-buffer we need to copy the data in two parts to a temp buffer
            // and then run the convert pass from that data
            state.temp_gen[1][0..rem_count].copy_from_slice(&state.ring_buffer[read_index..]);
            state.temp_gen[1][rem_count..bytes_size].copy_from_slice(&state.ring_buffer[0..rest_count]);

            unsafe {
                (state.output_resampler.plugin.convert.unwrap())(state.output_resampler.user_data,
                    dest.as_mut_ptr() as _, 
                    state.temp_gen[1].as_mut_ptr() as _, 
                    required_input_frames as _);
            }

            state.read_index.set(rest_count);
            state.read_index.bump_generation();
        }
    } else if (read_index + output_bytes_size) < ring_buffer_len {
        dest.copy_from_slice(&state.ring_buffer[read_index..read_index + output_bytes_size]);
        state.read_index.add(output_bytes_size);
    } else {
        let rem_count = state.ring_buffer[read_index..].len();
        let rest_count = output_bytes_size - rem_count; 
        // if we need to wrap the ring-buffer we need to copy the data in two parts
        dest[0..rem_count].copy_from_slice(&state.ring_buffer[read_index..]);
        dest[rem_count..output_bytes_size].copy_from_slice(&state.ring_buffer[0..rest_count]);
        state.read_index.set(rest_count);
        state.read_index.bump_generation();
    }

    let _ = msg.send(PlaybackReply::Data(dest));
}

/// Handles incoming messages (usually from the main thread)
fn incoming_msg(state: &mut PlaybackInternal, msg: &PlaybackMessage) {
    match msg {
        PlaybackMessage::QueuePlayback(playback, msg) => {
            // ponytail: single-file case — reset the frame stamp when the queue
            // was empty; a deeper queue keeps the active plugin's count.
            if state.players.is_empty() {
                state.output_frame = 0;
                *state.viz_snapshot.lock() = None;
            }
            // Capture scope buffers during read_data if the plugin supports it.
            if let Some(f) = playback.plugin.scope_enable {
                f(playback.user_data, true);
            }
            state.players.push((playback.clone(), msg.clone()));
        },

        PlaybackMessage::GetData(format, frames, msg) => {
            get_data(state, *format, *frames, msg);
        }

        PlaybackMessage::GetTrackerPosition(msg) => {
            if state.players.is_empty() {
                let _ = msg.send(PlaybackReply::NoData);
                return;
            }

            let player = &state.players[0].0;
            let mut output_data = [0u8; 8];
            unsafe { (player.plugin.event.unwrap())(player.user_data, output_data.as_mut_ptr(), 8) };
            let pos = u64::from_le_bytes(output_data);
            let _ = msg.send(PlaybackReply::TrackerPosition(pos));
        }
    }
}

fn copy_buffer_to_ring(state: &mut PlaybackInternal, frame_count: usize, buffer_index: usize) {
    let ring_buffer_len = state.ring_buffer.len(); 
    let byte_size = get_byte_size_format(state.internal_format, frame_count);
    let write_index = state.write_index.get();
    let input_buffer = &state.temp_gen[buffer_index];

    // if read index + size is smaller than the ring buffer size we can just copy the range into the ring buffer
    if write_index + byte_size < ring_buffer_len {
        state.ring_buffer[write_index..write_index + byte_size].copy_from_slice(&input_buffer[0..byte_size]);
        state.write_index.add(byte_size);
    } else {
        let rem_count = state.ring_buffer[write_index..].len();
        let rest_count = byte_size - rem_count; 
        // if we need to wrap the ring-buffer we need to copy the data in two parts
        state.ring_buffer[write_index..].copy_from_slice(&input_buffer[..rem_count]);
        state.ring_buffer[0..rest_count].copy_from_slice(&input_buffer[rem_count..byte_size]);
        state.write_index.set(rest_count);
        state.write_index.bump_generation();
    }
}

fn update(state: &mut PlaybackInternal) -> bool {
    if state.players.is_empty() {
        return true;
    }

    let ring_size = state.ring_buffer.len();
    // get the read offset adjusted w
    let write_index = state.write_index.value;

    let read_cmp = if (state.read_index.get() + ring_size / 2) > ring_size {
        let diff = (state.read_index.get() + ring_size / 2) - ring_size; 
        (state.read_index.value + (1 << 32u64)) & 0xffff_ffff_0000_0000 | diff as u64
    } else {
        state.read_index.value + ring_size as u64 / 2
    };

    // if write index is larger than read_index + half the size of the ring buffer we don't generate any more data
    if write_index > read_cmp {
        return true;
    }

    // Owned so the viz snapshot can read the plugin while `state` is mutated below.
    let player = state.players[0].0.clone();

    // ponytail: fixed 1024-frame decode chunk at the internal format; make the
    // chunk size and format configurable if a plugin needs it.
    let read_info = ReadInfo {
        format: state.internal_format,
        frame_count: 1024,
        status: ReadStatus::DecodingRequest,
    };

    let read_data = ReadData {
        channels_output: state.temp_gen[0].as_mut_ptr() as _,
        channels_output_max_bytes_size: state.temp_gen[0].len() as _,
        info: read_info,
    };

    // Read data from the plugin
    let info = unsafe { (player.plugin.read_data.unwrap())(player.user_data, read_data) };

    // Interleaved with read_data on this (decode) thread: stamp the new output
    // position and copy the plugin's viz getters into a value-semantic snapshot
    // for the UI thread. ponytail: rebuilt every decode chunk; throttle if a
    // profile says it matters.
    state.output_frame += info.frame_count as u64;
    if let Some(snapshot) = build_snapshot(&player.plugin, player.user_data, state.output_frame) {
        *state.viz_snapshot.lock() = Some(snapshot);
    }

    // can just copy the data to the ringbuffer
    if info.format == state.internal_format {
        copy_buffer_to_ring(state, info.frame_count as _, 0);
    } else {
        // reconfigure the plugin resampler whenever the plugin's output format changes
        if state.plugin_format != info.format {
            let config = ConvertConfig { input: info.format, output: state.internal_format };
            unsafe { (state.plugin_resampler.plugin.set_config.unwrap())(state.plugin_resampler.user_data, &config) };
            state.plugin_format = info.format;
        }

        let required_input_frames = unsafe { 
            (state.plugin_resampler.plugin.get_required_input_frame_count.unwrap())(
                state.plugin_resampler.user_data,
                info.frame_count as _) 
        };

        // if read are within the ring-buffer range we can just convert directly from it to the output
        let frame_count = unsafe {
            (state.plugin_resampler.plugin.convert.unwrap())(state.plugin_resampler.user_data,
                state.temp_gen[1].as_mut_ptr() as _, 
                state.temp_gen[0].as_mut_ptr() as _, 
                required_input_frames as _)
        };

        copy_buffer_to_ring(state, frame_count as _, 1);
    }

    // sanity check that ring-buffer read isn't larger than write read
    if state.read_index.value >= state.write_index.value {
        error!("ring-buffer read is ahead of write (read: {:x} write: {:x})", state.read_index.value, state.write_index.value);
    }

    // info check if we have finished reading from this plugin and if that is the case we will close it and remove it from the player list
    if info.status == ReadStatus::Finished {
        let player = &state.players[0].0;
        let _ = state.players[0].1.send(PlaybackReply::PlaybackEnded);
        unsafe { (player.plugin.destroy.unwrap())(player.user_data) };
        state.players.remove(0);
        trace!("Playback finished - players left {}", state.players.len());
    }

    false
}

impl Playback {
    pub fn new(resample_plugins: ResamplePlugins) -> Result<Playback> {
        let viz_snapshot: VizSnapshotSlot = Arc::new(Mutex::new(None));
        let state = PlaybackInternal::new(resample_plugins, viz_snapshot.clone())?;
        Self::spawn(state, viz_snapshot)
    }

    /// Build a playback with caller-supplied resampler instances and an empty
    /// plugin registry. Only used by tests, which fabricate the resamplers
    /// instead of dlopen-ing real plugins.
    #[cfg(test)]
    fn new_with_resamplers(
        output_resampler: ResamplePluginInstance,
        plugin_resampler: ResamplePluginInstance,
    ) -> Result<Playback> {
        let viz_snapshot: VizSnapshotSlot = Arc::new(Mutex::new(None));
        let registry = Arc::new(parking_lot::RwLock::new(Vec::new()));
        let state = PlaybackInternal::with_resamplers(registry, output_resampler, plugin_resampler, viz_snapshot.clone());
        Self::spawn(state, viz_snapshot)
    }

    fn spawn(mut state: PlaybackInternal, viz_snapshot: VizSnapshotSlot) -> Result<Playback> {
        let (channel, thread_recv) = unbounded::<PlaybackMessage>();

        // Setup worker thread
        thread::Builder::new()
            .name("playback".to_string())
            .spawn(move || {
                loop {
                    if let Ok(msg) = thread_recv.try_recv() {
                        incoming_msg(&mut state, &msg);
                    }

                    if update(&mut state) {
                        thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
            })?;

        trace!("Playback create: done");

        Ok(Playback { channel, viz_snapshot })
    }

}

pub fn get_byte_size_format(format: AudioFormat, frames: usize) -> usize {
    let stream_size = match format.audio_format {
        AudioStreamFormat::U8 => 1,
        AudioStreamFormat::S16 => 2,
        AudioStreamFormat::S24 => 3,
        AudioStreamFormat::S32 => 4,
        AudioStreamFormat::F32 => 4,
    };

    stream_size * format.channel_count as usize * frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::ptr;

    fn fake_name() -> *const core::ffi::c_char {
        b"fake\0".as_ptr() as *const core::ffi::c_char
    }

    // --- fabricated identity resampler (input == output == DEFAULT_AUDIO_FORMAT) ---

    extern "C" fn r_set_config(_ud: *mut c_void, _cfg: *const ConvertConfig) {}
    extern "C" fn r_passthru(_ud: *mut c_void, n: u32) -> u32 { n }
    extern "C" fn r_convert(_ud: *mut c_void, out: *mut c_void, inp: *mut c_void, n: u32) -> u32 {
        let bytes = get_byte_size_format(DEFAULT_AUDIO_FORMAT, n as usize);
        unsafe { ptr::copy_nonoverlapping(inp as *const u8, out as *mut u8, bytes) };
        n
    }

    fn noop_resampler() -> ResamplePluginInstance {
        ResamplePluginInstance {
            user_data: ptr::null_mut(),
            plugin: ResamplePlugin {
                api_version: plugin_types::RV_RESAMPLE_PLUGIN_API_VERSION,
                name: fake_name(),
                version: fake_name(),
                library_version: fake_name(),
                create: None,
                destroy: None,
                set_config: Some(r_set_config),
                convert: Some(r_convert),
                get_expected_output_frame_count: Some(r_passthru),
                get_required_input_frame_count: Some(r_passthru),
                static_init: None,
                settings_updated: None,
            },
        }
    }

    fn make_internal() -> PlaybackInternal {
        let registry = Arc::new(parking_lot::RwLock::new(Vec::new()));
        PlaybackInternal::with_resamplers(
            registry,
            noop_resampler(),
            noop_resampler(),
            Arc::new(Mutex::new(None)),
        )
    }

    // --- fabricated decoder: each read_data writes 1024 stereo f32 frames as a
    //     sample-index ramp (sample k -> k.0) and never finishes ---

    const FAKE_FRAMES: usize = 1024;

    extern "C" fn d_read_data(_ud: *mut c_void, dest: ReadData) -> ReadInfo {
        let samples = FAKE_FRAMES * DEFAULT_AUDIO_FORMAT.channel_count as usize;
        let out = dest.channels_output as *mut f32;
        for k in 0..samples {
            unsafe { *out.add(k) = k as f32 };
        }
        ReadInfo { format: DEFAULT_AUDIO_FORMAT, frame_count: FAKE_FRAMES as u32, status: ReadStatus::Ok }
    }
    extern "C" fn d_destroy(_ud: *mut c_void) -> i32 { 0 }

    fn fake_decoder() -> PlaybackPluginInstance {
        PlaybackPluginInstance {
            user_data: ptr::null_mut(),
            plugin: PlaybackPlugin {
                api_version: plugin_types::RV_PLAYBACK_PLUGIN_API_VERSION,
                name: fake_name(),
                version: fake_name(),
                library_version: fake_name(),
                probe_can_play: None,
                supported_extensions: None,
                create: None,
                destroy: Some(d_destroy),
                event: None,
                open: None,
                close: None,
                read_data: Some(d_read_data),
                seek: None,
                metadata: None,
                static_init: None,
                settings_updated: None,
                static_destroy: None,
                viz_info: None,
                tracker_columns: None,
                tracker_channels: None,
                scope_channels: None,
                tracker_position: None,
                tracker_channel_rows: None,
                tracker_cells: None,
                scope_enable: None,
                scope_samples: None,
                vu_levels: None,
            },
        }
    }

    #[test]
    fn index_generation_and_offset() {
        let mut i = Index::default();
        i.set(5);
        assert_eq!(i.get(), 5);
        i.add(3);
        assert_eq!(i.get(), 8);
        let before = i.value;
        i.bump_generation();
        assert_eq!(i.value, before + (1 << 32));
        assert_eq!(i.get(), 8, "bump_generation must not touch the low offset");
        i.set(2);
        assert_eq!(i.get(), 2);
        assert_eq!(i.value >> 32, 1, "set must keep the generation");
    }

    #[test]
    fn byte_size_format_per_sample_format() {
        // F32 stereo: 4 bytes * 2 ch
        assert_eq!(get_byte_size_format(DEFAULT_AUDIO_FORMAT, 512), 4096);
        assert_eq!(get_byte_size_format(DEFAULT_AUDIO_FORMAT, 0), 0);
        let fmt = |f| AudioFormat { audio_format: f, channel_count: 1, sample_rate: 48000 };
        assert_eq!(get_byte_size_format(fmt(AudioStreamFormat::U8), 10), 10);
        assert_eq!(get_byte_size_format(fmt(AudioStreamFormat::S16), 10), 20);
        assert_eq!(get_byte_size_format(fmt(AudioStreamFormat::S24), 10), 30);
        assert_eq!(get_byte_size_format(fmt(AudioStreamFormat::S32), 10), 40);
        assert_eq!(get_byte_size_format(fmt(AudioStreamFormat::F32), 10), 40);
    }

    #[test]
    fn get_data_direct_copy() {
        let mut st = make_internal();
        st.ring_buffer = (0..64u32).map(|i| i as u8).collect();
        st.read_index.value = 0;
        st.write_index.value = 64;

        let (tx, rx) = unbounded();
        get_data(&mut st, DEFAULT_AUDIO_FORMAT, 2, &tx); // 2 frames = 16 bytes

        match rx.recv().unwrap() {
            PlaybackReply::Data(d) => assert_eq!(&d[..], &(0..16u8).collect::<Vec<_>>()[..]),
            _ => panic!("expected Data"),
        }
        assert_eq!(st.read_index.get(), 16);
    }

    #[test]
    fn get_data_wraps_ring() {
        let mut st = make_internal();
        st.ring_buffer = (0..64u32).map(|i| i as u8).collect();
        st.read_index.value = 56;
        // write far ahead (next generation) so the data guard passes
        st.write_index.value = (1 << 32) | 16;

        let (tx, rx) = unbounded();
        get_data(&mut st, DEFAULT_AUDIO_FORMAT, 2, &tx); // 16 bytes, wraps at 64

        match rx.recv().unwrap() {
            PlaybackReply::Data(d) => {
                let mut expected: Vec<u8> = (56..64).collect();
                expected.extend(0..8u8);
                assert_eq!(&d[..], &expected[..]);
            }
            _ => panic!("expected Data"),
        }
        assert_eq!(st.read_index.get(), 8);
        assert_eq!(st.read_index.value >> 32, 1, "wrap must bump the generation");
    }

    #[test]
    fn copy_buffer_to_ring_direct_and_wrap() {
        let mut st = make_internal();
        st.ring_buffer = vec![0u8; 64];
        for k in 0..16 {
            st.temp_gen[0][k] = (100 + k) as u8;
        }

        // direct: write at 0, 16 bytes fits
        copy_buffer_to_ring(&mut st, 2, 0);
        assert_eq!(&st.ring_buffer[0..16], &(100..116u8).collect::<Vec<_>>()[..]);
        assert_eq!(st.write_index.get(), 16);

        // wrap: write at 56, 16 bytes spans the end
        st.write_index = Index::default();
        st.write_index.value = 56;
        copy_buffer_to_ring(&mut st, 2, 0);
        assert_eq!(&st.ring_buffer[56..64], &(100..108u8).collect::<Vec<_>>()[..]);
        assert_eq!(&st.ring_buffer[0..8], &(108..116u8).collect::<Vec<_>>()[..]);
        assert_eq!(st.write_index.get(), 8);
        assert_eq!(st.write_index.value >> 32, 1, "wrap must bump the generation");
    }

    // Headless end-to-end: fake decoder -> decode thread -> ring -> GetData,
    // no audio device, no real plugin .so.
    #[test]
    fn headless_pipeline_flows_frames() {
        let playback = Playback::new_with_resamplers(noop_resampler(), noop_resampler())
            .expect("spawn playback");
        playback.queue_playback(fake_decoder()).expect("queue");

        let frames = 512usize;
        let want_bytes = get_byte_size_format(DEFAULT_AUDIO_FORMAT, frames);

        // Poll until the decode thread has filled enough ring data.
        let mut data = None;
        for _ in 0..2000 {
            let (tx, rx) = unbounded();
            playback
                .channel
                .send(PlaybackMessage::GetData(DEFAULT_AUDIO_FORMAT, frames, tx))
                .unwrap();
            match rx.recv().unwrap() {
                PlaybackReply::Data(d) => {
                    data = Some(d);
                    break;
                }
                PlaybackReply::NoData => thread::sleep(std::time::Duration::from_millis(1)),
                other => panic!("unexpected reply: {:?}", PlaybackReplyName(&other)),
            }
        }

        let data = data.expect("decode thread never produced frames");
        assert_eq!(data.len(), want_bytes, "wrong byte count back from pipeline");

        // First read starts at ring offset 0, so it returns the decoder's first
        // samples: the ramp 0.0, 1.0, 2.0, ...
        let floats: Vec<f32> = data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        assert_eq!(floats[0], 0.0);
        assert_eq!(floats[1023], 1023.0);
        assert!(floats.windows(2).all(|w| w[1] > w[0]), "ramp must be strictly increasing");
    }

    // PlaybackReply has no Debug; name just the variant for panic messages.
    struct PlaybackReplyName<'a>(&'a PlaybackReply);
    impl std::fmt::Debug for PlaybackReplyName<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            let s = match self.0 {
                PlaybackReply::PlaybackEnded => "PlaybackEnded",
                PlaybackReply::NoData => "NoData",
                PlaybackReply::TrackerPosition(_) => "TrackerPosition",
                PlaybackReply::Data(_) => "Data",
            };
            f.write_str(s)
        }
    }
}

