use plugin_types::{
    PlaybackPlugin, 
    ResamplePlugin,
    AudioFormat, 
    AudioStreamFormat, 
    ReadData, 
    ReadInfo, 
    ReadStatus, ConvertConfig
};
use crossbeam_channel::{Sender, unbounded};
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

impl PlaybackSettings {
    fn new() -> PlaybackSettings {
        // 2000 ms of buffering and approx max 90% cpu load
        PlaybackSettings { buffer_len_ms: 2000, max_cpu_load: 90 }
    }
}

// Temp buffer size is 1 sec of audio data for 2 channels floats
const TEMP_BUFFER_SIZE: usize = 48000 * 4 * 2;
const DEFAULT_AUDIO_FORMAT: AudioFormat = AudioFormat {
    audio_format: AudioStreamFormat::F32,
    channel_count: 1,
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


#[derive(Clone)]
pub struct PlaybackPluginInstance {
    pub user_data: *mut c_void,
    pub plugin: PlaybackPlugin,
}

#[derive(Clone)]
pub struct ResamplePluginInstance {
    pub user_data: *mut c_void,
    pub plugin: ResamplePlugin,
}


pub struct PlaybackInternal {
    pub players: Vec<PlaybackPluginInstance>,
    /// List of all resample plugins
    pub resample_plugins: ResamplePlugins,
    /// Used for generating data to requests
    pub output_resampler: ResamplePluginInstance,
    /// Resampler when reading data from plugins 
    pub plugin_resampler: ResamplePluginInstance, 
    /// Temporary buffer used when requesting data from a player plugin
    temp_gen: Vec<u8>,
    temp_resample: Vec<u8>,
    /// Ring buffer for audio output
    ring_buffer: Vec<u8>,
    read_index: usize,
    write_index: usize,
    write_generation: u32,
    read_generation: u32,
    // Format used for the internal ring-buffer
    internal_format: AudioFormat,
    /// Format used for the get_data requester.
    /// TODO: Keep a cache of these?
    last_request_format: AudioFormat,
}

pub enum PlaybackMessage {
    QueuePlayback(PlaybackPluginInstance),
    GetData(plugin_types::AudioFormat, usize, Sender<PlaybackReply>)
}

pub enum PlaybackReply {
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
            temp_gen: vec![0u8; TEMP_BUFFER_SIZE],
            temp_resample: vec![0u8; TEMP_BUFFER_SIZE],
            players: Vec::new(),
            resample_plugins,
            output_resampler,
            plugin_resampler,
            read_index: 0,
            write_index: 0,
            read_generation: 0,
            write_generation: 0,
            internal_format: DEFAULT_AUDIO_FORMAT,
            last_request_format: DEFAULT_AUDIO_FORMAT, 
        })
    }

    fn create_default_resample_plugin(resample_plugis: &ResamplePlugins) -> Result<ResamplePluginInstance> {
        let resample_plugins = resample_plugis.read();

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
    }

    if state.read_index == state.write_index && state.read_generation == state.write_generation {
        msg.send(PlaybackReply::NoData).unwrap();
        return;
    }


    // TODO: Verify that that requested size is reasonable
    let byte_size = get_byte_size_format(format, frames);
    let ring_buffer_len = state.ring_buffer.len(); 

    // TODO: Uninit
    let mut dest = vec![0u8; byte_size].into_boxed_slice();

    let read_index = state.read_index;

    // if format differs from the default format we need to convert it
    if format != DEFAULT_AUDIO_FORMAT {
        trace!("converting from {:?} -> {:?}", DEFAULT_AUDIO_FORMAT, format);

        let required_input_frames = unsafe { 
            (state.output_resampler.plugin.get_required_input_frame_count)(state.output_resampler.user_data, frames as _) 
        };

        let bytes_size = get_byte_size_format(format, required_input_frames as _);

        // if read are within the ring-buffer range we can just convert directly from it to the output
        if read_index + bytes_size < ring_buffer_len {
            unsafe {
                (state.output_resampler.plugin.convert)(state.output_resampler.user_data, 
                    dest.as_mut_ptr() as _, 
                    state.ring_buffer[read_index..].as_mut_ptr() as _, 
                    frames as _);
            }
            state.read_index += bytes_size;
        } else {
            let rem_count = state.ring_buffer[read_index..].len();
            let rest_count = byte_size - rem_count; 
            // if we need to wrap the ring-buffer we need to copy the data in two parts to a temp buffer
            // and then run the convert pass from that data
            state.temp_resample[0..rem_count].copy_from_slice(&state.ring_buffer[read_index..]);
            state.temp_resample[rem_count..byte_size].copy_from_slice(&state.ring_buffer[0..rest_count]);

            unsafe {
                (state.output_resampler.plugin.convert)(state.output_resampler.user_data, 
                    dest.as_mut_ptr() as _, 
                    state.ring_buffer[read_index..].as_mut_ptr() as _, 
                    frames as _);
            }

            state.read_index = rest_count;
            state.read_generation = state.read_generation.wrapping_add(1);
        }

    } else {
        trace!("Requesting data {}:{} - {}:{}", 
            state.read_index, state.read_generation,
            state.write_index, state.write_generation);

        if read_index + byte_size < ring_buffer_len {
            dest.copy_from_slice(&state.ring_buffer[read_index..read_index + byte_size]);
            state.read_index += byte_size;
        } else {
            let rem_count = state.ring_buffer[read_index..].len();
            let rest_count = byte_size - rem_count; 
            // if we need to wrap the ring-buffer we need to copy the data in two parts
            dest[0..rem_count].copy_from_slice(&state.ring_buffer[read_index..]);
            dest[rem_count..byte_size].copy_from_slice(&state.ring_buffer[0..rest_count]);
            state.read_index = rest_count;
            state.read_generation = state.read_generation.wrapping_add(1);
        }
    }

    msg.send(PlaybackReply::Data(dest)).unwrap();
} 

/// Handles incoming messages (usually from the main thread)
fn incoming_msg(state: &mut PlaybackInternal, msg: &PlaybackMessage) {
    match msg {
        // TODO: Implement
        PlaybackMessage::QueuePlayback(playback) => {
            state.players.push(playback.clone());
        },

        PlaybackMessage::GetData(format, frames, msg) => {
            get_data(state, *format, *frames, msg);
        }
    }
}

fn update(state: &mut PlaybackInternal) -> bool {
    if state.players.is_empty() {
        return true;
    }

    // get the read offset adjusted w
    let gen = state.write_generation - state.read_generation;
    let write_index = state.read_index + state.ring_buffer.len() * gen as usize;

    // if write index is larger than read_index + half the size of the ring buffer we don't  generate any more data 
    if write_index > (state.read_index + state.ring_buffer.len() / 2) {
        //trace!("Write is twice as large as read index, no extra data generated");
        return true;
    }

    let player = &state.players[0];

    // TODO: Use configured audio format
    // TODO: Fix hard-coded frames-count
    let read_info = ReadInfo {
        format: state.internal_format,
        frame_count: 1024,
        status: ReadStatus::DecodingRequest,
        virtual_channel_count: 0,
    };

    let read_data = ReadData {
        channels_output: state.temp_gen.as_mut_ptr() as _,
        virtual_channel_output: ptr::null_mut(),
        channels_output_max_bytes_size: state.temp_gen.len() as _,
        virtual_channels_output_max_bytes_size: 0,
        info: read_info,
    };

    let info = unsafe { (player.plugin.read_data)(player.user_data, read_data) };
    let ring_buffer_len = state.ring_buffer.len(); 

    trace!("write_index {}:{}", state.write_index, state.write_generation);

    // can just copy the data to the ringbuffer
    if info.format == state.internal_format {
        let byte_size = get_byte_size_format(info.format, info.frame_count as _);
        let write_index = state.write_index;
        // if read index + size is smaller than the ring buffer size we can just copy the range into the ring buffer
        if write_index + byte_size < ring_buffer_len {
            state.ring_buffer[write_index..write_index + byte_size].copy_from_slice(&state.temp_gen[0..byte_size]);
            state.write_index += byte_size;
        } else {
            let rem_count = state.ring_buffer[write_index..].len();
            let rest_count = byte_size - rem_count; 
            // if we need to wrap the ring-buffer we need to copy the data in two parts
            state.ring_buffer[write_index..].copy_from_slice(&state.temp_gen[..rem_count]);
            state.ring_buffer[0..rest_count].copy_from_slice(&state.temp_gen[rem_count..byte_size]);
            state.write_index = rest_count;
            state.write_generation = state.write_generation.wrapping_add(1);
        }

        // sanity check that ring-buffer read isn't larger than write read
        if state.read_index >= state.write_index && state.read_generation == state.write_generation {
            error!("ring-buffer read is ahead of write (read: {} write: {})", state.read_index, state.write_index);
        }

    } else {
        todo!("This needs to be implemented!");
    }

    false
}

impl Playback {
    pub fn new(resample_plugins: ResamplePlugins) -> Result<Playback> {
        let (channel, thread_recv) = unbounded::<PlaybackMessage>();

        let mut state = PlaybackInternal::new(resample_plugins)?;

        trace!("Playback create");

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


/*
pub struct Playback {
    /// for sending messages to the main-thread
    main_send: crossbeam_channel::Sender<PlaySendMsg>,
}



pub fn start_playback_thread() {
    let (data_send, thread_rec) = unbounded::<SendMsg>();

    // Setup worker thread
    thread::Builder::new()
        .name("playback".to_string())
        .spawn(move || {
            let mut state = VfsState::new();

            while let Ok(msg) = thread_recv.recv() {
                handle_msg(&mut state, "vfs_worker", &msg);
            }
        })
        .unwrap();

    Vfs { main_send }
}
*/




/*
#[inline]
fn format_in_bytes(format: u32) -> usize {
    match format as InputType {
        InputType::Unknown => 0,
        InputType::U8 => 1,
        InputType::S16 => 2,
        InputType::S24 => 3,
        InputType::S32 => 4,
        InputType::F32 => 4,
    }
}

fn output_callback(
    user_data: *mut c_void,
    output_ptr: *mut c_void,
    sample_rate: u32,
    channels: u32,
    format: u32,
    frames: u32,
) {
    let playback: &mut DataCallback = unsafe { &mut *(user_data as *mut DataCallback) };

    let playback;

    {
        let mut pb = data.playback.lock().unwrap();
        if pb.players.len() == 0 {
            return;
        }

        playback = pb.players[0].clone();

        if playback.is_paused {
            return;
        }

        // if we have some settings copy them over
    }

    // calculate the output frame size
    let frame_count = frame_count as usize;
    let format_size_bytes = format_in_bytes(format);
    let channel_count = channels as usize;
    let frame_stride = format_size_bytes * channel_count;
    let frames_to_read = frame_count * frame_stride;
    let output_sample_rate = sample_rate as _;

    if frames_to_read == 0 {
        info!("Frames to read is zero, which isn't legal. Please try to report this error");
        return;
    }

    let output = unsafe { slice::from_raw_parts_mut(output_ptr as *mut u8, frames_to_read) };

    //debug!("frames to read {} -------------------- ", frames_to_read);

    // if we have decoded enough data we can just copy it
    if (data.read_index + frames_to_read) <= data.frames_decoded {
        //debug!("[COPY ALL]  Remaining from last offset {} size {} ", data.read_index, frames_to_read);
        output.copy_from_slice(&data.mix_buffer[data.read_index..data.read_index + frames_to_read]);
        data.read_index += frames_to_read;
        return;
    }
    // else we need to copy what we have left, decode new frame(s) and put that into the output buffer
    let diff = data.frames_decoded - data.read_index;

    if diff != 0 {
        let read_end = data.read_index + diff;
        //debug!("[COPY]     Remaining from last offset {} size {} ", data.read_index, diff);
        // copy the remaining stored data
        output[0..diff].copy_from_slice(&data.mix_buffer[data.read_index..read_end]);
    }

    let mut write_offset = diff;

    //println!("[WRITE ST] {}", write_offset);

    // Start produce new frames to fill up the whole buffer
    loop {
        let data_left = frames_to_read - write_offset;

        //println!("data left to generate {} (bytes) frames {}", data_left, data_left / frame_stride);

        let info = ((playback.plugin.plugin_funcs).read_data)(
            playback.plugin_user_data as *mut c_void,
            data.temp_gen.as_mut_ptr() as *mut _,
            data.temp_gen.len() as u32,
            output_sample_rate,
        );

        //let read_format = Format::from_c(info.output_format as u32);
        let frames_read = info.sample_count; // * info.channel_count as u16;

        // TODO: proper handling of this
        if frames_read == 0 {
            break;
        }

        //println!("updating converter with channel count {}, format {:#?} sample rate {}",
        //	info.channel_count, read_format, info.sample_rate);

        // update the data converter with the current format (will re-init if needed)
        //data.converter
        //    .update(info.channel_count, read_format, info.sample_rate);

        // We calculate how how much data we will generate with the converter.
        //let frames_out = data.converter.expected_output_frame_count(frames_read as _) as usize;
        let expected_output = frames_out * frame_stride;

        //println!("[GEN]      Expected output frames {} from input {}", frames_out, frames_read);

        // if we are about to generate more frames than we have place for in the output buffer
        // we generate them to a temporary mix buffer, copy the part we need and will copy the rest during the next update.
        if expected_output >= data_left {
            /*
            data.converter
                .process_pcm_frames(
                    data.mix_buffer.as_mut_ptr() as *mut _,
                    data.temp_gen.as_ptr() as *const _,
                    frames_out,
                    frames_read as _,
                )
                .unwrap();
            */
            //debug!("{:#?}", p);

            //output[write_offset..].copy_from_slice(&data.mix_buffer[0..data_left]);
            output[write_offset..].copy_from_slice(&data.temp_gen[0..data_left]);
            data.read_index = data_left;
            data.frames_decoded = expected_output;

            //debug!("[GEN BR]   Generate to temp size {}", expected_output);
            //debug!("[GEN BR]   Copy frome slice to offset {} - len {}", write_offset, data_left);

            break;
        } else {
            //debug!("[GEN]      Generate to output offset {} - size {}", write_offset, expected_output);

            // if here we haven't filled up the buffer just yet, copy what we have and processed to decode another frame
            let offset_end = write_offset + expected_output;

            /*
            data.converter
                .process_pcm_frames(
                    output[write_offset..offset_end].as_mut_ptr() as *mut _,
                    data.temp_gen.as_ptr() as *const _,
                    frames_out,
                    frames_read as _,
                )
                .unwrap();
             */
            output[write_offset..offset_end].copy_from_slice(&data.temp_gen);

            write_offset += expected_output;
        }
    }
    */

/*
let mut file = std::fs::OpenOptions::new()
    .write(true)
    .append(true)
    .open("/home/emoon/temp/dump.raw")
    .unwrap();

file.write_all(&output).unwrap();
*/
//}

/*
use logger::*;
use messages::*;
use miniaudio::{Device, Devices};
use std::time::Instant;

use crate::ffi::{HSSetting, HippoSettingsUpdate_Default};
use crate::playback_settings;
use crate::plugin_handler::DecoderPlugin;
use miniaudio::{DataConverter, DataConverterConfig, Format, ResampleAlgorithm};
use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::Mutex;

use crate::service_ffi::PluginService;
//use ringbuf::{Consumer, Producer, RingBuffer};
use ringbuf::{Producer, RingBuffer};

const DEFAULT_DEVICE_NAME: &str = "Default Sound Device";

#[derive(Clone)]
pub struct HippoPlayback {
    plugin_user_data: u64,
    pub plugin: DecoderPlugin,
    is_paused: bool,
}

pub struct Playback {
    pub players: Vec<HippoPlayback>,
    pub updated_settings: Vec<HSSetting>,
    pub settings_active: bool,
    pub updated_time: Option<Instant>,
}

pub struct DataCallback {
    pub playback: Mutex<Playback>,
    pub updated_settings: *const Vec<HSSetting>,
    mix_buffer: Vec<u8>,
    temp_gen: Vec<u8>,
    read_index: usize,
    frames_decoded: usize,
    converter: DataConverter,
}

impl DataCallback {
    fn new() -> Box<DataCallback> {
        let input_output_channels = 2;
        let input_output_sample_rate = 48000;
        let input_output_format = Format::S16;

        let cfg = DataConverterConfig::new(
            Format::F32,
            input_output_format,
            input_output_channels,
            input_output_channels,
            input_output_sample_rate,
            input_output_sample_rate,
            ResampleAlgorithm::Linear {
                lpf_order: 1,
                lpf_nyquist_factor: 1.0,
            }, //ResampleAlgorithm::Speex { quality: 3 },
        );

        let update_settings = unsafe { Box::new(vec![std::mem::zeroed::<HSSetting>(); 256]) };

        Box::new(DataCallback {
            playback: Mutex::new(Playback {
                players: Vec::<HippoPlayback>::new(),
                updated_settings: Vec::with_capacity(256),
                settings_active: false,
                updated_time: None,
            }),
            mix_buffer: vec![0; 48000 * 32 * 4], // mix buffer of 48k, 32ch * int, should be enough for temp
            temp_gen: vec![0; 48000 * 32 * 4], // mix buffer of 48k, 32ch * int, should be enough for temp
            read_index: 0,
            frames_decoded: 0,
            updated_settings: Box::into_raw(update_settings),
            converter: DataConverter::new(&cfg).unwrap(),
        })
    }
}

pub struct Instance {
    _plugin_user_data: u64,
    _plugin: DecoderPlugin,
    pub write_stream: Producer<Box<[u8]>>,
}

impl HippoPlayback {
    pub fn start_with_file(
        plugin: &DecoderPlugin,
        plugin_service: &PluginService,
        filename: &str,
    ) -> Option<(HippoPlayback, Instance)> {
        let c_filename;
        let subsong_index;
        // Find subsong separator
        // TODO: store subsong index instead?
        if let Some(separator) = filename.find('|') {
            // create filename without separator
            c_filename = CString::new(&filename[..separator]).unwrap();
            subsong_index = *&filename[separator + 1..].parse::<i32>().unwrap();
        } else {
            c_filename = CString::new(filename).unwrap();
            subsong_index = 0i32;
        }

        let user_data =
            unsafe { ((plugin.plugin_funcs).create)(plugin_service.get_c_service_api()) } as u64;
        let ptr_user_data = user_data as *mut c_void;

        let ps = crate::service_ffi::get_playback_settings(plugin_service.c_service_api);
        ps.selected_id = (plugin.plugin_funcs).name.to_owned();

        let settings_api =
            crate::service_ffi::get_playback_settings_c(plugin_service.c_service_api);

        //let frame_size = (((plugin.plugin_funcs).frame_size)(ptr_user_data)) as usize;
        let open_state = unsafe {
            ((plugin.plugin_funcs).open)(
                ptr_user_data,
                c_filename.as_ptr(),
                subsong_index,
                settings_api,
            )
        };

        if open_state < 0 {
            return None;
        }

        let rb = RingBuffer::<Box<[u8]>>::new(256);
        let (prod, _cons) = rb.split();

        Some((
            HippoPlayback {
                plugin_user_data: user_data,
                plugin: plugin.clone(),
                is_paused: false,
                //_read_stream: cons,
            },
            Instance {
                write_stream: prod,
                _plugin_user_data: user_data,
                _plugin: plugin.clone(),
            },
        ))
    }
}
pub struct HippoAudio {
    pub device_name: String,
    pub data_callback: *mut c_void,
    output_device: Option<Device>,
    output_devices: Option<Devices>,
    pub playbacks: Vec<Instance>,
}

unsafe extern "C" fn data_callback(
    device_ptr: *mut miniaudio::ma_device,
    output_ptr: *mut c_void,
    _input_ptr: *const c_void,
    frame_count: u32,
) {
    let data: &mut DataCallback = std::mem::transmute((*device_ptr).pUserData);
    let mut settings_data: *const Vec<HSSetting> = std::ptr::null_mut();
    let playback;

    {
        // miniaudio will clear the buffer so we don't have to do it here
        let mut pb = data.playback.lock().unwrap();
        if pb.players.len() == 0 {
            return;
        }

        playback = pb.players[0].clone();

        if playback.is_paused {
            return;
        }

        // if we have some settings copy them over

        if pb.settings_active {
            settings_data = data.updated_settings;
            let dest_data: &mut Vec<HSSetting> = std::mem::transmute(data.updated_settings);

            let mut len = pb.updated_settings.len();
            // TODO: Constant
            if len >= 256 {
                warn!("Settings has over 256 entries! clamping");
                len = 255;
            }

            dest_data[0..len].copy_from_slice(&pb.updated_settings[0..len]);
            pb.settings_active = false;
        }
    }

    // now we can apply settings to the playback if we have any

    if settings_data != std::ptr::null_mut() {
        let callback = playback_settings::get_threaded_callback(settings_data as *const _);
        if let Some(update_callback) = (playback.plugin.plugin_funcs).settings_updated {
            if (update_callback)(
                playback.plugin_user_data as *mut c_void,
                &callback as *const _,
            ) == HippoSettingsUpdate_Default
            {
                // No need to track time if we live tweak everything
                let mut pb = data.playback.lock().unwrap();
                pb.updated_time = None;
            } else {
                println!("setting tweak requires restart");
            }
        }
    }

    // calculate the output frame size
    let cfg = data.converter.config();
    let frame_count = frame_count as usize;
    let format_size_bytes = Format::from_c(cfg.formatOut).size_in_bytes();
    let channel_count = cfg.channelsOut as usize;
    let frame_stride = format_size_bytes * channel_count;
    let frames_to_read = frame_count * frame_stride;
    let output_sample_rate = cfg.sampleRateOut;

    let output = std::slice::from_raw_parts_mut(output_ptr as *mut u8, frames_to_read);

    //println!("frames to read {} -------------------- ", frames_to_read);

    // if we have decoded enough data we can just copy it
    if (data.read_index + frames_to_read) <= data.frames_decoded {
        //println!("[COPY ALL]  Remaining from last offset {} size {} ", data.read_index, frames_to_read);
        output.copy_from_slice(&data.mix_buffer[data.read_index..data.read_index + frames_to_read]);
        data.read_index += frames_to_read;
        return;
    }
    // else we need to copy what we have left, decode new frame(s) and put that into the output buffer
    let diff = data.frames_decoded - data.read_index;

    if diff != 0 {
        let read_end = data.read_index + diff;
        //println!("[COPY]     Remaining from last offset {} size {} ", data.read_index, diff);
        // copy the remaining stored data
        output[0..diff].copy_from_slice(&data.mix_buffer[data.read_index..read_end]);
    }

    let mut write_offset = diff;

    //println!("[WRITE ST] {}", write_offset);

    // Start produce new frames to fill up the whole buffer
    loop {
        let data_left = frames_to_read - write_offset;

        //println!("data left to generate {} (bytes) frames {}", data_left, data_left / frame_stride);

        let info = ((playback.plugin.plugin_funcs).read_data)(
            playback.plugin_user_data as *mut c_void,
            data.temp_gen.as_mut_ptr() as *mut _,
            data.temp_gen.len() as u32,
            output_sample_rate,
        );

        let read_format = Format::from_c(info.output_format as u32);
        let frames_read = info.sample_count; // * info.channel_count as u16;

        // TODO: proper handling of this
        if frames_read == 0 {
            break;
        }

        //println!("updating converter with channel count {}, format {:#?} sample rate {}",
        //	info.channel_count, read_format, info.sample_rate);

        // update the data converter with the current format (will re-init if needed)
        data.converter
            .update(info.channel_count, read_format, info.sample_rate);

        // We calculate how how much data we will generate with the converter.
        let frames_out = data.converter.expected_output_frame_count(frames_read as _) as usize;
        let expected_output = frames_out * frame_stride;

        //println!("[GEN]      Expected output frames {} from input {}", frames_out, frames_read);

        // if we are about to generate more frames than we have place for in the output buffer
        // we generate them to a temporary mix buffer, copy the part we need and will copy the rest during the next update.
        if expected_output >= data_left {
            data.converter
                .process_pcm_frames(
                    data.mix_buffer.as_mut_ptr() as *mut _,
                    data.temp_gen.as_ptr() as *const _,
                    frames_out,
                    frames_read as _,
                )
                .unwrap();
            //println!("{:#?}", p);

            output[write_offset..].copy_from_slice(&data.mix_buffer[0..data_left]);
            data.read_index = data_left;
            data.frames_decoded = expected_output;

            //println!("[GEN BR]   Generate to temp size {}", expected_output);
            //println!("[GEN BR]   Copy frome slice to offset {} - len {}", write_offset, data_left);

            break;
        } else {
            //println!("[GEN]      Generate to output offset {} - size {}", write_offset, expected_output);

            // if here we haven't filled up the buffer just yet, copy what we have and processed to decode another frame
            let offset_end = write_offset + expected_output;

            data.converter
                .process_pcm_frames(
                    output[write_offset..offset_end].as_mut_ptr() as *mut _,
                    data.temp_gen.as_ptr() as *const _,
                    frames_out,
                    frames_read as _,
                )
                .unwrap();

            write_offset += expected_output;
        }
    }

    /*
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .append(true)
        .open("/home/emoon/temp/dump.raw")
        .unwrap();

    file.write_all(&output).unwrap();
    */
}

impl HippoAudio {
    pub fn new() -> HippoAudio {
        // This is a bit hacky so it can be shared with the device and HippoAudio
        let data_callback = DataCallback::new();

        HippoAudio {
            device_name: DEFAULT_DEVICE_NAME.to_owned(),
            data_callback: Box::into_raw(data_callback) as *mut c_void,
            output_devices: None,
            output_device: None,
            playbacks: Vec::new(),
        }
    }

    pub fn stop(&mut self) {
        // TODO: Call playback destruction of data
        let data_callback: &DataCallback = unsafe { std::mem::transmute(self.data_callback) };
        let mut pb = data_callback.playback.lock().unwrap();
        pb.players.clear();
        self.playbacks.clear();
    }

    fn select_output_device(
        &mut self,
        msg: &HippoSelectOutputDevice,
    ) -> Result<(), miniaudio::Error> {
        let name = msg.name().unwrap();
        self.init_device(name)?;
        self.device_name = name.to_owned();
        Ok(())
    }

    fn reply_output_devices(&self) -> Option<Box<[u8]>> {
        let output_devices = self.output_devices.as_ref()?;

        let mut builder = messages::FlatBufferBuilder::new_with_capacity(8192);
        let mut out_ent = Vec::with_capacity(output_devices.devices.len());

        let device_name = builder.create_string(&self.device_name);

        for dev in &output_devices.devices {
            let device_name = builder.create_string(&dev.name);

            let desc = HippoOutputDevice::create(
                &mut builder,
                &HippoOutputDeviceArgs {
                    name: Some(device_name),
                    min_channels: dev.min_channels as i32,
                    max_channels: dev.max_channels as i32,
                    min_sample_rate: dev.min_sample_rate as i32,
                    max_sample_rate: dev.max_channels as i32,
                },
            );

            out_ent.push(desc);
        }

        let devices_vec = builder.create_vector(&out_ent);

        let added_devices = HippoReplyOutputDevices::create(
            &mut builder,
            &HippoReplyOutputDevicesArgs {
                current_device: Some(device_name),
                devices: Some(devices_vec),
            },
        );

        Some(HippoMessage::create_def(
            builder,
            MessageType::reply_output_devices,
            added_devices.as_union_value(),
        ))
    }

    fn request_select_song(&mut self, msg: &HippoMessage) -> Option<Box<[u8]>> {
        let select_song = msg.message_as_request_select_song().unwrap();
        let pause = select_song.pause_state();
        let force = select_song.force();

        let data_callback: &DataCallback = unsafe { std::mem::transmute(self.data_callback) };
        let mut pb = data_callback.playback.lock().unwrap();

        if pb.players.len() == 1 {
            pb.players[0].is_paused = pause;

            if force {
                pb.players[0].is_paused = false;
            }
        }

        None
    }

    ///
    /// Handle incoming events
    ///
    pub fn event(&mut self, msg: &HippoMessage) -> Option<Box<[u8]>> {
        match msg.message_type() {
            MessageType::request_select_song => self.request_select_song(msg),
            MessageType::request_output_devices => self.reply_output_devices(),
            MessageType::select_output_device => {
                trace!("Trying to select new output from UI");
                let select_output = msg.message_as_select_output_device().unwrap();
                if let Err(e) = self.select_output_device(&select_output) {
                    error!("Unable to select output device {:#?}", e);
                }
                None
            }
            _ => None,
        }
    }

    pub fn init_devices(&mut self) -> Result<(), miniaudio::Error> {
        self.output_devices = Some(Devices::new()?);
        Ok(())
    }

    fn init_default_device(&mut self) -> Result<(), miniaudio::Error> {
        let context = self.output_devices.as_ref().unwrap().context;

        self.output_device = Some(Device::new(
            data_callback,
            self.data_callback,
            context,
            None,
        )?);

        Ok(())
    }

    pub fn close_device(&mut self) {
        if let Some(ref mut device) = self.output_device.as_ref() {
            device.close();
        }

        self.output_device = None;
    }

    pub fn init_device(&mut self, playback_device: &str) -> Result<(), miniaudio::Error> {
        if self.output_devices.is_none() {
            self.output_devices = Some(Devices::new()?);
        }

        let output_devices = self.output_devices.as_ref().unwrap();
        let context = output_devices.context;

        if playback_device == DEFAULT_DEVICE_NAME {
            self.close_device();
            self.init_default_device()?;
        } else {
            for device in &output_devices.devices {
                let device_id = device.device_id;
                if device.name == playback_device {
                    self.close_device();
                    self.output_device = Some(Device::new(
                        data_callback,
                        self.data_callback,
                        context,
                        Some(&device_id),
                    )?);
                    break;
                }
            }
        }

        let device = self.output_device.as_ref().unwrap();
        println!(
            "output device rate {} format {:#?} channels {}",
            device.sample_rate(),
            device.format(),
            device.channels()
        );

        let data_callback: &mut DataCallback = unsafe { std::mem::transmute(self.data_callback) };
        data_callback.converter.update_output(
            device.channels() as u8,
            device.format(),
            device.sample_rate(),
        );

        device.start()
    }

    //pub fn pause(&mut self) {
    //    self.audio_sink.pause();
    //}

    //pub fn play(&mut self) {
    //   self.audio_sink.play();
    //}

    pub fn start_with_file(
        &mut self,
        plugin: &DecoderPlugin,
        service: &PluginService,
        filename: &str,
    ) -> bool {
        if self.output_device.is_none() || self.output_devices.is_none() {
            error!(
                "Unable to play {} because system has no audio device(s)",
                filename
            );
            return false;
        }

        // TODO: Do error checking
        let playback = HippoPlayback::start_with_file(plugin, service, filename);

        if let Some(pb) = playback {
            let data_callback: &DataCallback = unsafe { std::mem::transmute(self.data_callback) };
            let mut t = data_callback.playback.lock().unwrap();

            if t.players.len() == 1 {
                t.players[0] = pb.0;
            } else {
                t.players.push(pb.0);
            }

            self.playbacks.push(pb.1);

            return true;
        }

        return false;
    }
}

use crate::ffi;
use dynamic_reload::{DynamicReload, Lib, PlatformName, Search, Symbol};
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::Arc;
use walkdir::{DirEntry, WalkDir};

//use hippo_api::ffi::{CHippoPlaybackPlugin};
use crate::service_ffi::PluginService;
use crate::service_ffi::ServiceApi;
use logger::*;

#[derive(Debug, Clone)]
pub struct HippoPlaybackPluginFFI {
    pub api_version: u64,
    pub user_data: u64, // this is really a pointer but Rust gets sad when we use this on another thread so we hack it here a bit.
    pub name: String,
    pub version: String,
    pub library_version: String,
    pub probe_can_play: unsafe extern "C" fn(
        data: *const u8,
        data_size: u32,
        filename: *const c_char,
        total_size: u64,
    ) -> u32,
    pub supported_extensions: unsafe extern "C" fn() -> *const c_char,
    pub create: unsafe extern "C" fn(services: *const ffi::HippoServiceAPI) -> *mut c_void,
    pub destroy: unsafe extern "C" fn(user_data: *mut c_void) -> i32,
    pub event: Option<unsafe extern "C" fn(user_data: *mut c_void, data: *const u8, len: i32)>,

    pub open: unsafe extern "C" fn(user_data: *mut c_void, buffer: *const c_char, subsong: i32, *const ffi::HippoSettingsAPI) -> i32,
    pub close: unsafe extern "C" fn(user_data: *mut c_void) -> i32,
    pub read_data: unsafe extern "C" fn(
        user_data: *mut c_void,
        dest: *mut c_void,
        max_sample_count: u32,
        native_sample_rate: u32,
    ) -> ffi::HippoReadInfo,
    pub seek: unsafe extern "C" fn(user_data: *mut c_void, ms: i32) -> i32,
    pub metadata: Option<
        unsafe extern "C" fn(buffer: *const i8, services: *const ffi::HippoServiceAPI) -> i32,
    >,
    pub settings_updated: Option<
        unsafe extern "C" fn(user_data: *mut c_void, settings_api: *const ffi::HippoSettingsAPI) -> u32,
    >,
}

#[derive(Clone)]
pub struct DecoderPlugin {
    pub plugin: Arc<Lib>,
    pub plugin_path: String,
    pub plugin_funcs: HippoPlaybackPluginFFI,
}

#[cfg(target_os = "macos")]
pub fn get_plugin_ext() -> &'static str {
    "dylib"
}

#[cfg(target_os = "linux")]
pub fn get_plugin_ext() -> &'static str {
    "so"
}

#[cfg(target_os = "windows")]
pub fn get_plugin_ext() -> &'static str {
    "dll"
}

impl DecoderPlugin {
    pub fn probe_can_play(
        &self,
        data: &[u8],
        buffer_len: usize,
        filename: &str,
        file_size: u64,
    ) -> bool {
        let c_filename = CString::new(filename).unwrap();
        let res = unsafe {
            ((self.plugin_funcs).probe_can_play)(
                data.as_ptr(),
                buffer_len as u32,
                c_filename.as_ptr(),
                file_size,
            )
        };

        match res {
            0 => true,
            _ => false,
        }
    }

    pub fn get_metadata(&self, filename: &str, service: &PluginService) {
        let c_filename = CString::new(filename).unwrap();
        unsafe {
            if let Some(func) = self.plugin_funcs.metadata {
                let _ = func(c_filename.as_ptr(), service.get_c_service_api());
            }
        };
    }
}

pub struct Plugins {
    pub decoder_plugins: Vec<Box<DecoderPlugin>>,
    pub plugin_handler: DynamicReload,
}

impl Plugins {
    pub fn new() -> Plugins {
        Plugins {
            decoder_plugins: Vec::new(),
            plugin_handler: DynamicReload::new(Some(vec!["."]), None, Search::Default),
        }
    }

    fn add_plugin_lib(&mut self, name: &str, plugin: &Arc<Lib>, service_api: *const ffi::HippoServiceAPI) {
        let func: Result<
            Symbol<extern "C" fn() -> *const ffi::HippoPlaybackPlugin>,
            ::std::io::Error,
        > = unsafe { plugin.lib.get(b"hippo_playback_plugin\0") };

        if let Ok(fun) = func {
            let native_plugin = unsafe { *fun() };

            // To make the plugin code a bit nicer we move over to a separate structure internally.
            // This also allows us allows us to check functions are correct at one place instead of
            // having unwraps of function ptrs for every call
            let plugin_funcs = HippoPlaybackPluginFFI {
                api_version: native_plugin.api_version,
                user_data: 0,
                name: unsafe {
                    CStr::from_ptr(native_plugin.name)
                        .to_string_lossy()
                        .into_owned()
                },
                version: unsafe {
                    CStr::from_ptr(native_plugin.version)
                        .to_string_lossy()
                        .into_owned()
                },
                library_version: unsafe {
                    CStr::from_ptr(native_plugin.library_version)
                        .to_string_lossy()
                        .into_owned()
                },
                probe_can_play: native_plugin.probe_can_play.unwrap(),
                supported_extensions: native_plugin.supported_extensions.unwrap(),
                create: native_plugin.create.unwrap(),
                event: native_plugin.event,
                destroy: native_plugin.destroy.unwrap(),
                open: native_plugin.open.unwrap(),
                close: native_plugin.close.unwrap(),
                read_data: native_plugin.read_data.unwrap(),
                seek: native_plugin.seek.unwrap(),
                metadata: native_plugin.metadata,
                settings_updated: native_plugin.settings_updated,
            };

            trace!("Loaded playback plugin {} {}", plugin_funcs.name, plugin_funcs.version);

            if let Some(static_init) = native_plugin.static_init {
                // TODO: Memory leak
                let name = format!("{} {}", plugin_funcs.name, plugin_funcs.version);
                let c_name = CString::new(name).unwrap();
                let log_api = Box::into_raw(ServiceApi::create_log_api());

                unsafe {
                    (*log_api).log_set_base_name.unwrap()((*log_api).priv_data, c_name.as_ptr());
                    (static_init)(log_api, service_api);
                }
            }

            self.decoder_plugins.push(Box::new(DecoderPlugin {
                plugin: plugin.clone(),
                plugin_path: name.to_owned(),
                plugin_funcs,
            }));
        }
    }

    fn check_file_type(entry: &DirEntry) -> bool {
        let path = entry.path();

        if let Some(ext) = path.extension() {
            ext == get_plugin_ext()
        } else {
            false
        }
    }

    fn internal_add_plugins_from_path(&mut self, path: &str, service_api: *const ffi::HippoServiceAPI) {
        for entry in WalkDir::new(path).max_depth(1) {
            if let Ok(t) = entry {
                if Self::check_file_type(&t) {
                    self.add_plugin(t.path().to_str().unwrap(), service_api);
                    //println!("{}", t.path().display());
                }
            }
        }
    }

    pub fn add_plugins_from_path(&mut self, service_api: *const ffi::HippoServiceAPI) {
        self.internal_add_plugins_from_path("plugins", service_api);
        self.internal_add_plugins_from_path(".", service_api);
    }

    pub fn add_plugin(&mut self, name: &str, service_api: *const ffi::HippoServiceAPI) {
        match self.plugin_handler.add_library(name, PlatformName::No) {
            Ok(lib) => self.add_plugin_lib(name, &lib, service_api),
            Err(e) => {
                println!("Unable to load dynamic lib, err {:?}", e);
            }
        }
    }
}



*/

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

