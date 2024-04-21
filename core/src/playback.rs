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
use std::{
    ptr,
    thread,
    os::raw::c_void,
};

use crate::plugin_handler::ResamplePlugins;

#[derive(Default)]
pub struct PlaybackSettings {
    /// How many ms to pre-buffer
    pub buffer_len_ms: usize,
    /// Max CPU load in percent on the decoder thread
    pub max_cpu_load: usize,
}

/*
impl PlaybackSettings {
    fn new() -> PlaybackSettings {
        // 2000 ms of buffering and approx max 90% cpu load
        PlaybackSettings { buffer_len_ms: 2000, max_cpu_load: 90 }
    }
}
*/

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
    /// TODO: Keep a cache of these?
    last_request_format: AudioFormat,
}

pub enum PlaybackMessage {
    QueuePlayback(PlaybackPluginInstance, Sender<PlaybackReply>),
    GetData(plugin_types::AudioFormat, usize, Sender<PlaybackReply>)
}

pub enum PlaybackReply {
    /// Playback of requsted file has started
    PlaybackStarted,
    /// Playback of the request has ended
    PlaybackEnded,
    /// This reply can happen if the decoder thread hasn't generated enough data.
    OutOfData,
    /// Generated if the request is invalid (i.e too large size etc) 
    InvalidRequest,
    /// This will happen if no data has been generated yet 
    NoData,
    /// Returns data back to the requster
    Data(Box<[u8]>),
}

impl PlaybackInternal {
    fn new(resample_plugins: ResamplePlugins) -> Result<PlaybackInternal> {
        let output_resampler = Self::create_default_resample_plugin(&resample_plugins)?;
        let plugin_resampler = Self::create_default_resample_plugin(&resample_plugins)?;

        // TODO: Should be passed in
        //let settings = PlaybackSettings::new();
        let ring_buffer_size = get_byte_size_format(DEFAULT_AUDIO_FORMAT, DEFAULT_AUDIO_FORMAT.sample_rate as usize * 2);
        Ok(PlaybackInternal { 
            //settings,
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
        })
    }

    fn create_default_resample_plugin(resample_plugins: &ResamplePlugins) -> Result<ResamplePluginInstance> {
        let resample_plugins = resample_plugins.read();

        if resample_plugins.is_empty() {
            bail!("No resample plugin(s) found. Unable to setup Retrovert playback");
        }
        
        let op = &resample_plugins[0];

        let plugin_name = op.plugin_funcs.get_name();
        let service_funcs = op.service.get_c_api();
        let user_data = unsafe { ((op.plugin_funcs).create)(service_funcs) };

        if user_data.is_null() {
            bail!("{} : unable to allocate instance", plugin_name);
        }
        
        let config = plugin_types::ConvertConfig { input: DEFAULT_AUDIO_FORMAT, output: DEFAULT_AUDIO_FORMAT };

        unsafe { (op.plugin_funcs.set_config)(user_data, &config); }
        
        trace!("Created default resample plugin: {}", plugin_name);

        Ok(ResamplePluginInstance { user_data, plugin: op.plugin_funcs })
    }
}

// Send back data to the requester if possible
fn get_data(state: &mut PlaybackInternal, format: AudioFormat, frames: usize, msg: &Sender<PlaybackReply>) {
    // Update format if it differs
    if state.last_request_format != format {
        let config = ConvertConfig { input: DEFAULT_AUDIO_FORMAT, output: format };
        unsafe { (state.output_resampler.plugin.set_config)(state.output_resampler.user_data, &config) };
        state.last_request_format = format;
    }

    if state.read_index >= state.write_index {
        msg.send(PlaybackReply::NoData).unwrap();
        return;
    }

    // TODO: Verify that that requested size is reasonable
    let output_bytes_size = get_byte_size_format(format, frames);
    let ring_buffer_len = state.ring_buffer.len(); 

    // if we haven't generated any data yet
    if output_bytes_size as u64 > state.write_index.value {
        msg.send(PlaybackReply::NoData).unwrap();
        return;
    }

    // TODO: Uninit
    let mut dest = vec![0u8; output_bytes_size].into_boxed_slice();

    let read_index = state.read_index.get();

    // if format differs from the default format we need to convert it
    if format != DEFAULT_AUDIO_FORMAT {
        //trace!("converting from {:?} -> {:?}", DEFAULT_AUDIO_FORMAT, format);

        let required_input_frames = unsafe { 
            (state.output_resampler.plugin.get_required_input_frame_count)(state.output_resampler.user_data, frames as _) 
        };

        let bytes_size = get_byte_size_format(DEFAULT_AUDIO_FORMAT, required_input_frames as _);

        // if read are within the ring-buffer range we can just convert directly from it to the output
        if (read_index + bytes_size) < ring_buffer_len {
            unsafe {
                (state.output_resampler.plugin.convert)(state.output_resampler.user_data, 
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
                (state.output_resampler.plugin.convert)(state.output_resampler.user_data, 
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

    msg.send(PlaybackReply::Data(dest)).unwrap();
} 

/// Handles incoming messages (usually from the main thread)
fn incoming_msg(state: &mut PlaybackInternal, msg: &PlaybackMessage) {
    match msg {
        // TODO: Implement
        PlaybackMessage::QueuePlayback(playback, msg) => {
            state.players.push((playback.clone(), msg.clone()));
        },

        PlaybackMessage::GetData(format, frames, msg) => {
            get_data(state, *format, *frames, msg);
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

    // TODO: Fix this code, it's really ugly
    let read_cmp = if (state.read_index.get() + ring_size / 2) > ring_size {
        let diff = (state.read_index.get() + ring_size / 2) - ring_size; 
        (state.read_index.value + (1 << 32u64)) & 0xffff_ffff_0000_0000 | diff as u64
    } else {
        state.read_index.value + ring_size as u64 / 2
    };

    // if write index is larger than read_index + half the size of the ring buffer we don't  generate any more data 
    if write_index > read_cmp {
        //trace!("Write is twice as large as read index, no extra data generated");
        return true;
    }

    let player = &state.players[0].0;

    // TODO: Use configured audio format
    // TODO: Fix hard-coded frames-count
    let read_info = ReadInfo {
        format: state.internal_format,
        frame_count: 1024,
        status: ReadStatus::DecodingRequest,
        virtual_channel_count: 0,
    };

    let read_data = ReadData {
        channels_output: state.temp_gen[0].as_mut_ptr() as _,
        virtual_channel_output: ptr::null_mut(),
        channels_output_max_bytes_size: state.temp_gen[0].len() as _,
        virtual_channels_output_max_bytes_size: 0,
        info: read_info,
    };

    // Read data from the plugin
    let info = unsafe { (player.plugin.read_data)(player.user_data, read_data) };

    // can just copy the data to the ringbuffer
    if info.format == state.internal_format {
        copy_buffer_to_ring(state, info.frame_count as _, 0);
    } else {
        // make sure 
        if state.plugin_format != info.format {
            dbg!(state.internal_format);
            dbg!(info.format);
            let config = ConvertConfig { input: info.format, output: state.internal_format };
            unsafe { (state.plugin_resampler.plugin.set_config)(state.plugin_resampler.user_data, &config) };
            state.plugin_format = info.format;
        }

        let required_input_frames = unsafe { 
            (state.plugin_resampler.plugin.get_required_input_frame_count)(
                state.plugin_resampler.user_data, 
                info.frame_count as _) 
        };

        // if read are within the ring-buffer range we can just convert directly from it to the output
        let frame_count = unsafe {
            (state.plugin_resampler.plugin.convert)(state.plugin_resampler.user_data, 
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
        state.players[0].1.send(PlaybackReply::PlaybackEnded).unwrap();
        unsafe { (player.plugin.destroy)(player.user_data) };
        state.players.remove(0);
        trace!("Playback finished - players left {}", state.players.len());
    }

    false
}

impl Playback {
    pub fn new(resample_plugins: ResamplePlugins) -> Result<Playback> {
        let (channel, thread_recv) = unbounded::<PlaybackMessage>();

        let mut state = PlaybackInternal::new(resample_plugins)?;

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

        Ok(Playback { channel })
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

