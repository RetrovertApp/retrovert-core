use services::*;
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::slice;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum InputType {
    Unknown = 0,
    U8 = 1,
    S16 = 2,
    S24 = 3,
    S32 = 4,
    F32 = 5,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct WriteInfo {
    pub sample_rate: u32,
    pub sample_count: u16,
    pub channel_count: u8,
    pub output_format: u8,
}

impl WriteInfo {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct PlaybackCallback {
    pub user_data: *mut c_void,
    pub callback: unsafe extern "C" fn(
        user_data: *mut c_void,
        data: *mut c_void,
        sample_rate: u32,
        channels: u32,
        format: u32,
        frames: u32,
    ),
}

impl PlaybackCallback {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct OutputTargets {
    pub names: *const *const c_char,
    pub names_size: u64,
}

impl OutputTargets {
    pub fn get_names(&self) -> &[*const c_char] {
        unsafe { slice::from_raw_parts(self.names, self.names_size as _) }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct OutputPlugin {
    pub api_version: u64,
    pub name: *const c_char,
    pub version: *const c_char,
    pub library_version: *const c_char,
    pub create: unsafe extern "C" fn(services: *const ServiceFFI) -> *mut c_void,
    pub destroy: unsafe extern "C" fn(user_data: *mut c_void) -> i32,
    pub output_targets_info: unsafe extern "C" fn(user_data: *mut c_void) -> OutputTargets,
    pub start: unsafe extern "C" fn(user_data: *mut c_void, callback: *mut PlaybackCallback),
    pub stop: unsafe extern "C" fn(user_data: *mut c_void),
    pub static_init: unsafe extern "C" fn(services: *const ServiceFFI),
}

impl OutputPlugin {
    pub fn get_name(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.name) };
        t.to_string_lossy()
    }
    pub fn get_version(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.version) };
        t.to_string_lossy()
    }
    pub fn get_library_version(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.library_version) };
        t.to_string_lossy()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum ProbeResult {
    Supported = 0,
    Unsupported = 1,
    Unsure = 2,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum OutputType {
    U8 = 1,
    S16 = 2,
    S24 = 3,
    S32 = 4,
    F32 = 5,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum PlaybackType {
    Tracker = 0,
    HardwareEmulated = 1,
    Streamed = 2,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum SettingsUpdate {
    Default = 0,
    RequireSongRestart = 1,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ReadInfo {
    pub sample_rate: u32,
    pub frame_count: u32,
    pub channel_count: u16,
    pub virtual_channel_count: u16,
    pub output_format: u16,
}

impl ReadInfo {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ReadData {
    pub channels_output: *mut c_void,
    pub virtual_channel_output: *mut c_void,
    pub channels_output_max_bytes_size: u32,
    pub virtual_channels_output_max_bytes_size: u32,
    pub info: ReadInfo,
}

impl ReadData {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct PlaybackInfo {
    pub virtual_channel_count: u32,
    pub playback_type: PlaybackType,
}

impl PlaybackInfo {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct PlaybackPlugin {
    pub api_version: u64,
    pub name: *const c_char,
    pub version: *const c_char,
    pub library_version: *const c_char,
    pub probe_can_play: unsafe extern "C" fn(
        data: *const u8,
        data_size: u64,
        filename: *const c_char,
        total_size: u64,
    ) -> ProbeResult,
    pub supported_extensions: unsafe extern "C" fn() -> *const c_char,
    pub create: unsafe extern "C" fn(services: *const ServiceFFI) -> *mut c_void,
    pub destroy: unsafe extern "C" fn(user_data: *mut c_void) -> i32,
    pub event: unsafe extern "C" fn(user_data: *mut c_void, data: *const u8, data_size: u64),
    pub open: unsafe extern "C" fn(
        user_data: *mut c_void,
        url: *const c_char,
        subsong: u32,
        settings: *const SettingsFFI,
    ) -> i32,
    pub close: unsafe extern "C" fn(user_data: *mut c_void),
    pub read_data: unsafe extern "C" fn(user_data: *mut c_void, dest: ReadData) -> ReadInfo,
    pub seek: unsafe extern "C" fn(user_data: *mut c_void, ms: i64) -> i64,
    pub metadata: unsafe extern "C" fn(url: *const c_char, services: *const ServiceFFI) -> i32,
    pub static_init: unsafe extern "C" fn(services: *const ServiceFFI),
    pub settings_updated: unsafe extern "C" fn(
        user_data: *mut c_void,
        settings: *const SettingsFFI,
    ) -> SettingsUpdate,
}

impl PlaybackPlugin {
    pub fn get_name(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.name) };
        t.to_string_lossy()
    }
    pub fn get_version(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.version) };
        t.to_string_lossy()
    }
    pub fn get_library_version(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.library_version) };
        t.to_string_lossy()
    }
}
