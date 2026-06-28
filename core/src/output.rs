use plugin_types::{AudioFormat, AudioStreamFormat};
use log::{error, trace};
use crossbeam_channel::{Sender, bounded};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use crate::playback::{Playback, PlaybackMessage, PlaybackReply, get_byte_size_format};

pub struct Output {
    playback_send: Sender<PlaybackMessage>,
    // Holds the cpal stream alive; dropping it stops audio.
    _stream: Option<cpal::Stream>,
}

fn as_bytes<T>(data: &mut [T]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u8, std::mem::size_of_val(data)) }
}

fn map_sample_format(fmt: SampleFormat) -> Option<AudioStreamFormat> {
    match fmt {
        SampleFormat::F32 => Some(AudioStreamFormat::F32),
        SampleFormat::I16 => Some(AudioStreamFormat::S16),
        _ => None,
    }
}

// Runs on cpal's audio thread; pulls from the decode thread, silence on a miss.
fn fill(channel: &Sender<PlaybackMessage>, format: AudioFormat, out: &mut [u8]) {
    let frames = out.len() / get_byte_size_format(format, 1);
    let (reply_send, reply_recv) = bounded::<PlaybackReply>(1);

    if channel.send(PlaybackMessage::GetData(format, frames, reply_send)).is_err() {
        out.fill(0);
        return;
    }

    match reply_recv.recv() {
        Ok(PlaybackReply::Data(data)) => {
            let n = data.len().min(out.len());
            out[..n].copy_from_slice(&data[..n]);
            out[n..].fill(0);
        }
        _ => out.fill(0),
    }
}

impl Output {
    pub fn new(playback: &Playback) -> Output {
        trace!("Output: created");
        Output { playback_send: playback.channel.clone(), _stream: None }
    }

    pub fn get_position(&mut self) -> u64 {
        let (playback_send, self_recv) = bounded::<PlaybackReply>(1);

        if self.playback_send.send(PlaybackMessage::GetTrackerPosition(playback_send)).is_err() {
            error!("Unable to communicate with playback, no position will be reported");
        }

        match self_recv.recv() {
            Ok(PlaybackReply::TrackerPosition(data)) => data,
            _ => 0,
        }
    }

    pub fn create_default_output(&mut self) {
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            error!("No default audio output device; audio will not play");
            return;
        };

        let supported = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => { error!("No default output config: {}; audio will not play", e); return; }
        };

        let sample_format = supported.sample_format();
        let Some(audio_format) = map_sample_format(sample_format) else {
            error!("Unsupported output sample format {:?}; audio will not play", sample_format);
            return;
        };

        let config: cpal::StreamConfig = supported.config();
        let format = AudioFormat {
            audio_format,
            channel_count: config.channels as u32,
            sample_rate: config.sample_rate.0,
        };

        let channel = self.playback_send.clone();
        let err_fn = |e| error!("cpal stream error: {}", e);

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| fill(&channel, format, as_bytes(data)),
                err_fn, None),
            SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| fill(&channel, format, as_bytes(data)),
                err_fn, None),
            _ => unreachable!("map_sample_format already rejected this"),
        };

        let stream = match stream {
            Ok(s) => s,
            Err(e) => { error!("Unable to build output stream: {}; audio will not play", e); return; }
        };

        if let Err(e) = stream.play() {
            error!("Unable to start output stream: {}; audio will not play", e);
            return;
        }

        trace!("Created cpal output: {:?} {} ch @ {} Hz", sample_format, format.channel_count, format.sample_rate);
        self._stream = Some(stream);
    }
}
