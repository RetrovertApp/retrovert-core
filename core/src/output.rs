use plugin_types::{OutputPlugin, PlaybackCallback, AudioFormat};
use std::os::raw::c_void;
use log::{error, trace};
use crossbeam_channel::{Sender, bounded};
use crate::playback::{Playback, PlaybackMessage, PlaybackReply};
use crate::plugin_handler::{OutputPlugins};

// This is called from output plugins. The purpose of it is that it will fetch data from the decoder thread.
// We use a separate thread for decoding as it makes it possible to buffer more data and to detect buffer underflow
unsafe extern "C" fn output_callback(user_data: *mut c_void, output_data: *mut c_void, format: AudioFormat, frames: u32) -> u32 {
    let callback: &mut OutputCallback = &mut *(user_data as *mut OutputCallback);

    let (playback_send, self_recv) = bounded::<PlaybackReply>(1);

    if callback.channel.send(PlaybackMessage::GetData(format, frames as _, playback_send)).is_err() {
        error!("Unable to communitate with playback, no data will be generated");
        return 0;
    }

    match self_recv.recv() {
        Ok(PlaybackReply::Data(data)) => {
            let total_size = crate::playback::get_byte_size_format(format, frames as usize);
            let output = std::slice::from_raw_parts_mut(output_data as *mut u8, total_size);
            output.copy_from_slice(&data);
            return frames;
        }
        //Ok(PlaybackReply::NoData) => trace!("No data has been generated yet"),
        Ok(PlaybackReply::InvalidRequest) => error!("Invalid request. No data was generated"),
        Ok(PlaybackReply::OutOfData) => error!("Ran out of data (likel requesting too fast/plaback is too slow. No data will be generated)"),
        Err(e) => error!("Got error when reading from playback {:?} No data will be generated.", e),
        _ => (),
    }

    0
}

#[derive(Clone)]
pub struct PluginOutput {
    pub user_data: *mut c_void,
    pub plugin: OutputPlugin,
}

pub struct OutputCallback {
    /// for sending messages to the main-thread
    channel: Sender<PlaybackMessage>,
}

pub struct Output {
    playback_send: Sender<PlaybackMessage>,
    output_plugins: OutputPlugins,
    current_output: Option<PluginOutput>, 
}

impl Output {
    pub fn new_callback(&self) -> *mut PlaybackCallback {
        let callback = OutputCallback { channel: self.playback_send.clone() };
        let callback_data = Box::leak(Box::new(callback));
        let ffi_callback = Box::leak(Box::new(
            PlaybackCallback {
                user_data: callback_data as *mut _ as *mut c_void,
                callback: output_callback,
            }
        ));

        ffi_callback as *mut _
    }

    pub fn new(playback: &Playback, output_plugins: OutputPlugins) -> Output {
        trace!("Output: created");
        Output { 
            playback_send: playback.channel.clone(),
            output_plugins,
            current_output: None, 
        }
    }

    pub fn create_default_output(&mut self) {
        // TODO: Error handling
        let output_plugs = self.output_plugins.read();

        let op = &output_plugs[0];

        let plugin_name = op.plugin_funcs.get_name();
        let service_funcs = op.service.get_c_api();
        let user_data = unsafe { ((op.plugin_funcs).create)(service_funcs) };

        if user_data.is_null() {
            error!("{} : unable to allocate instance, skipping playback", plugin_name); 
            return;
        }

        trace!("Created default output: {}", plugin_name);

        let callback = self.new_callback();
        unsafe { ((op.plugin_funcs).start)(user_data, callback) };

        self.current_output = Some(PluginOutput { user_data, plugin: op.plugin_funcs });

    }
}
