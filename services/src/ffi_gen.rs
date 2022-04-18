use crate::io::Io;
use crate::log::Log;
use crate::metadata::Metadata;
use crate::settings::Settings;
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::slice;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct IoReadUrlResult {
    pub data: *const u8,
    pub data_size: u64,
}

impl IoReadUrlResult {
    pub fn get_data(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.data, self.data_size as _) }
    }
}

extern "C" fn io_exists(self_c: *mut c_void, url: *const c_char) -> bool {
    let instance: &mut Io = unsafe { &mut *(self_c as *mut Io) };
    let url_ = unsafe { CStr::from_ptr(url) };
    let ret_val = instance.exists(&url_.to_string_lossy());
    ret_val
}

extern "C" fn io_read_url_to_memory(self_c: *mut c_void, url: *const c_char) -> IoReadUrlResult {
    let instance: &mut Io = unsafe { &mut *(self_c as *mut Io) };
    let url_ = unsafe { CStr::from_ptr(url) };
    let ret_val = instance.read_url_to_memory(&url_.to_string_lossy());
    ret_val
}

extern "C" fn io_free_url_to_memory(self_c: *mut c_void, memory: *mut core::ffi::c_void) {
    let instance: &mut Io = unsafe { &mut *(self_c as *mut Io) };
    instance.free_url_to_memory(memory)
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct IoFFI {
    pub private_data: *mut c_void,
    pub exists: unsafe extern "C" fn(self_c: *mut c_void, url: *const c_char) -> bool,
    pub read_url_to_memory:
        unsafe extern "C" fn(self_c: *mut c_void, url: *const c_char) -> IoReadUrlResult,
    pub free_url_to_memory:
        unsafe extern "C" fn(self_c: *mut c_void, memory: *mut core::ffi::c_void),
}

impl IoFFI {
    pub fn new(instance: *mut Io) -> IoFFI {
        IoFFI {
            private_data: instance as *mut c_void,
            exists: io_exists,
            read_url_to_memory: io_read_url_to_memory,
            free_url_to_memory: io_free_url_to_memory,
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LogFFI {
    pub private_data: *mut c_void,
    pub log: unsafe extern "C" fn(
        self_c: *mut c_void,
        level: u32,
        file: *const c_char,
        line: i32,
        fmt: *const c_char,
        ...
    ),
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum MetaEncoding {
    Utf8 = 0,
    ShiftJS2 = 1,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum RVMetadataResult {
    KeyNotFound = 0,
    UnableToMakeQuery = 1,
}
pub type MetadataId = u64;
extern "C" fn metadata_create_url(self_c: *mut c_void, url: *const c_char) -> MetadataId {
    let instance: &mut Metadata = unsafe { &mut *(self_c as *mut Metadata) };
    let url_ = unsafe { CStr::from_ptr(url) };
    let ret_val = instance.create_url(&url_.to_string_lossy());
    ret_val
}

extern "C" fn metadata_set_tag(
    self_c: *mut c_void,
    id: MetadataId,
    tag: *const c_char,
    data: *const c_char,
) {
    let instance: &mut Metadata = unsafe { &mut *(self_c as *mut Metadata) };
    let tag_ = unsafe { CStr::from_ptr(tag) };
    let data_ = unsafe { CStr::from_ptr(data) };
    instance.set_tag(id, &tag_.to_string_lossy(), &data_.to_string_lossy())
}

extern "C" fn metadata_set_tag_f64(
    self_c: *mut c_void,
    id: MetadataId,
    tag: *const c_char,
    data: f64,
) {
    let instance: &mut Metadata = unsafe { &mut *(self_c as *mut Metadata) };
    let tag_ = unsafe { CStr::from_ptr(tag) };
    instance.set_tag_f64(id, &tag_.to_string_lossy(), data)
}

extern "C" fn metadata_add_subsong(
    self_c: *mut c_void,
    parent_id: MetadataId,
    index: u32,
    name: *const c_char,
    length: f32,
) {
    let instance: &mut Metadata = unsafe { &mut *(self_c as *mut Metadata) };
    let name_ = unsafe { CStr::from_ptr(name) };
    instance.add_subsong(parent_id, index, &name_.to_string_lossy(), length)
}

extern "C" fn metadata_add_sample(self_c: *mut c_void, parent_id: MetadataId, text: *const c_char) {
    let instance: &mut Metadata = unsafe { &mut *(self_c as *mut Metadata) };
    let text_ = unsafe { CStr::from_ptr(text) };
    instance.add_sample(parent_id, &text_.to_string_lossy())
}

extern "C" fn metadata_add_instrument(
    self_c: *mut c_void,
    parent_id: MetadataId,
    text: *const c_char,
) {
    let instance: &mut Metadata = unsafe { &mut *(self_c as *mut Metadata) };
    let text_ = unsafe { CStr::from_ptr(text) };
    instance.add_instrument(parent_id, &text_.to_string_lossy())
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MetadataFFI {
    pub private_data: *mut c_void,
    pub create_url: unsafe extern "C" fn(self_c: *mut c_void, url: *const c_char) -> MetadataId,
    pub set_tag: unsafe extern "C" fn(
        self_c: *mut c_void,
        id: MetadataId,
        tag: *const c_char,
        data: *const c_char,
    ),
    pub set_tag_f64:
        unsafe extern "C" fn(self_c: *mut c_void, id: MetadataId, tag: *const c_char, data: f64),
    pub add_subsong: unsafe extern "C" fn(
        self_c: *mut c_void,
        parent_id: MetadataId,
        index: u32,
        name: *const c_char,
        length: f32,
    ),
    pub add_sample:
        unsafe extern "C" fn(self_c: *mut c_void, parent_id: MetadataId, text: *const c_char),
    pub add_instrument:
        unsafe extern "C" fn(self_c: *mut c_void, parent_id: MetadataId, text: *const c_char),
}

impl MetadataFFI {
    pub fn new(instance: *mut Metadata) -> MetadataFFI {
        MetadataFFI {
            private_data: instance as *mut c_void,
            create_url: metadata_create_url,
            set_tag: metadata_set_tag,
            set_tag_f64: metadata_set_tag_f64,
            add_subsong: metadata_add_subsong,
            add_sample: metadata_add_sample,
            add_instrument: metadata_add_instrument,
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub enum SettingsResult {
    Ok = 0,
    NotFound = 1,
    DuplicatedId = 2,
    WrongType = 3,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union Setting {
    pub int_value: SInteger,
    pub float_value: SFloat,
    pub int_fixed_value: SIntegerFixedRange,
    pub string_fixed_value: SStringFixedRange,
    pub bool_value: SBool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SBase {
    pub widget_id: *const c_char,
    pub name: *const c_char,
    pub desc: *const c_char,
    pub widget_type: i32,
}

impl SBase {
    pub fn get_widget_id(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.widget_id) };
        t.to_string_lossy()
    }
    pub fn get_name(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.name) };
        t.to_string_lossy()
    }
    pub fn get_desc(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.desc) };
        t.to_string_lossy()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SFloat {
    pub value: f32,
    pub start_range: f32,
    pub end_range: f32,
}

impl SFloat {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SInteger {
    pub value: i32,
    pub start_range: i32,
    pub end_range: i32,
}

impl SInteger {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SBool {
    pub value: bool,
}

impl SBool {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SIntegerRangeValue {
    pub name: *const c_char,
    pub value: i32,
}

impl SIntegerRangeValue {
    pub fn get_name(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.name) };
        t.to_string_lossy()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SStringRangeValue {
    pub name: *const c_char,
    pub value: *const c_char,
}

impl SStringRangeValue {
    pub fn get_name(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.name) };
        t.to_string_lossy()
    }
    pub fn get_value(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.value) };
        t.to_string_lossy()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SIntegerFixedRange {
    pub value: i32,
    pub values: *const SIntegerRangeValue,
    pub values_size: u64,
}

impl SIntegerFixedRange {
    pub fn get_values(&self) -> &[SIntegerRangeValue] {
        unsafe { slice::from_raw_parts(self.values, self.values_size as _) }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SStringFixedRange {
    pub value: *const c_char,
    pub values: *const SStringRangeValue,
    pub values_size: u64,
}

impl SStringFixedRange {
    pub fn get_value(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.value) };
        t.to_string_lossy()
    }
    pub fn get_values(&self) -> &[SStringRangeValue] {
        unsafe { slice::from_raw_parts(self.values, self.values_size as _) }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SIntResult {
    pub result: SettingsResult,
    pub value: i32,
}

impl SIntResult {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SFloatResult {
    pub result: SettingsResult,
    pub value: i32,
}

impl SFloatResult {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SStringResult {
    pub result: SettingsResult,
    pub value: *const c_char,
}

impl SStringResult {
    pub fn get_value(&self) -> Cow<str> {
        let t = unsafe { CStr::from_ptr(self.value) };
        t.to_string_lossy()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SBoolResult {
    pub result: SettingsResult,
    pub value: bool,
}

impl SBoolResult {}

extern "C" fn settings_reg(
    self_c: *mut c_void,
    name: *const c_char,
    settings: *const Setting,
    settings_size: u64,
) -> SettingsResult {
    let instance: &mut Settings = unsafe { &mut *(self_c as *mut Settings) };
    let name_ = unsafe { CStr::from_ptr(name) };
    let settings_ = unsafe { slice::from_raw_parts(settings, settings_size as _) };
    let ret_val = instance.reg(&name_.to_string_lossy(), &settings_);
    ret_val
}

extern "C" fn settings_get_string(
    self_c: *mut c_void,
    ext: *const c_char,
    id: *const c_char,
) -> SStringResult {
    let instance: &mut Settings = unsafe { &mut *(self_c as *mut Settings) };
    let ext_ = unsafe { CStr::from_ptr(ext) };
    let id_ = unsafe { CStr::from_ptr(id) };
    let ret_val = instance.get_string(&ext_.to_string_lossy(), &id_.to_string_lossy());
    ret_val
}

extern "C" fn settings_get_int(
    self_c: *mut c_void,
    ext: *const c_char,
    id: *const c_char,
) -> SIntResult {
    let instance: &mut Settings = unsafe { &mut *(self_c as *mut Settings) };
    let ext_ = unsafe { CStr::from_ptr(ext) };
    let id_ = unsafe { CStr::from_ptr(id) };
    let ret_val = instance.get_int(&ext_.to_string_lossy(), &id_.to_string_lossy());
    ret_val
}

extern "C" fn settings_get_float(
    self_c: *mut c_void,
    ext: *const c_char,
    id: *const c_char,
) -> SFloatResult {
    let instance: &mut Settings = unsafe { &mut *(self_c as *mut Settings) };
    let ext_ = unsafe { CStr::from_ptr(ext) };
    let id_ = unsafe { CStr::from_ptr(id) };
    let ret_val = instance.get_float(&ext_.to_string_lossy(), &id_.to_string_lossy());
    ret_val
}

extern "C" fn settings_get_bool(
    self_c: *mut c_void,
    ext: *const c_char,
    id: *const c_char,
) -> SBoolResult {
    let instance: &mut Settings = unsafe { &mut *(self_c as *mut Settings) };
    let ext_ = unsafe { CStr::from_ptr(ext) };
    let id_ = unsafe { CStr::from_ptr(id) };
    let ret_val = instance.get_bool(&ext_.to_string_lossy(), &id_.to_string_lossy());
    ret_val
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SettingsFFI {
    pub private_data: *mut c_void,
    pub reg: unsafe extern "C" fn(
        self_c: *mut c_void,
        name: *const c_char,
        settings: *const Setting,
        settings_size: u64,
    ) -> SettingsResult,
    pub get_string: unsafe extern "C" fn(
        self_c: *mut c_void,
        ext: *const c_char,
        id: *const c_char,
    ) -> SStringResult,
    pub get_int: unsafe extern "C" fn(
        self_c: *mut c_void,
        ext: *const c_char,
        id: *const c_char,
    ) -> SIntResult,
    pub get_float: unsafe extern "C" fn(
        self_c: *mut c_void,
        ext: *const c_char,
        id: *const c_char,
    ) -> SFloatResult,
    pub get_bool: unsafe extern "C" fn(
        self_c: *mut c_void,
        ext: *const c_char,
        id: *const c_char,
    ) -> SBoolResult,
}

impl SettingsFFI {
    pub fn new(instance: *mut Settings) -> SettingsFFI {
        SettingsFFI {
            private_data: instance as *mut c_void,
            reg: settings_reg,
            get_string: settings_get_string,
            get_int: settings_get_int,
            get_float: settings_get_float,
            get_bool: settings_get_bool,
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ServiceFFI {
    pub private_data: *mut c_void,
    pub get_io: unsafe extern "C" fn(pd: *mut c_void, api_version: u64) -> *const IoFFI,
    pub get_log: unsafe extern "C" fn(pd: *mut c_void, api_version: u64) -> *const LogFFI,
    pub get_metadata: unsafe extern "C" fn(pd: *mut c_void, api_version: u64) -> *const MetadataFFI,
    pub get_settings: unsafe extern "C" fn(pd: *mut c_void, api_version: u64) -> *const SettingsFFI,
}

pub struct ServiceApi {
    pub c_io_api: *const IoFFI,
    pub c_log_api: *const LogFFI,
    pub c_metadata_api: *const MetadataFFI,
    pub c_settings_api: *const SettingsFFI,
}

extern "C" fn get_io_api_wrapper(priv_data: *mut c_void, _version: u64) -> *const IoFFI {
    let service_api: &mut ServiceApi = unsafe { &mut *(priv_data as *mut ServiceApi) };
    service_api.c_io_api
}

extern "C" fn get_log_api_wrapper(priv_data: *mut c_void, _version: u64) -> *const LogFFI {
    let service_api: &mut ServiceApi = unsafe { &mut *(priv_data as *mut ServiceApi) };
    service_api.c_log_api
}

extern "C" fn get_metadata_api_wrapper(
    priv_data: *mut c_void,
    _version: u64,
) -> *const MetadataFFI {
    let service_api: &mut ServiceApi = unsafe { &mut *(priv_data as *mut ServiceApi) };
    service_api.c_metadata_api
}

extern "C" fn get_settings_api_wrapper(
    priv_data: *mut c_void,
    _version: u64,
) -> *const SettingsFFI {
    let service_api: &mut ServiceApi = unsafe { &mut *(priv_data as *mut ServiceApi) };
    service_api.c_settings_api
}

impl ServiceFFI {
    pub fn new(service_api: *const ServiceApi) -> ServiceFFI {
        ServiceFFI {
            private_data: service_api as *mut c_void,
            get_io: get_io_api_wrapper,
            get_log: get_log_api_wrapper,
            get_metadata: get_metadata_api_wrapper,
            get_settings: get_settings_api_wrapper,
        }
    }
}
