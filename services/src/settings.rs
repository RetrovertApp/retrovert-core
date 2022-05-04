use crate::ffi_gen::*;
use log::info;
use std::collections::{HashMap, HashSet};
use std::ptr;
use std::rc::Rc;
use std::slice;

#[derive(Clone, Serialize, Deserialize)]
enum SerValue {
    NoSetting,
    FloatValue(f32),
    IntValue(i32),
    BoolValue(bool),
    StrValue(String),
}

pub struct NativeSettings {
    native_settings: *const Setting,
    native_settings_count: usize,
    stored_settings: Vec<Setting>,
}

impl NativeSettings {
    pub fn new(settings: &[Setting]) -> NativeSettings {
        NativeSettings {
            native_settings: settings.as_ptr(),
            native_settings_count: settings.len(),
            stored_settings: settings.to_vec(),
        }
    }
}

pub struct PluginSettings {
    settings: HashMap<String, NativeSettings>,
}

pub struct Settings {
    native_settings: *const Setting,
    native_settings_count: usize,
    stored_settings: Vec<Setting>,
    existing_settings: Rc<HashSet<String>>,
}

impl Settings {
    pub fn new(existing_settings: &Rc<HashSet<String>>) -> Settings {
        Settings {
            native_settings: ptr::null(),
            native_settings_count: 0,
            stored_settings: Vec::new(),
            existing_settings: existing_settings.clone(),
        }
    }

    pub fn reg(&mut self, name: &str, settings: &[Setting]) -> SettingsResult {
        if let Some(_ps) = self.existing_settings.get(name) {
            info!("Trying to register settings for {} twice, skipping", name);
            SettingsResult::DuplicatedId
        } else {
            SettingsResult::Ok
        }
    }

    pub fn get_string(&mut self, _ext: &str, id: &str) -> SStringResult {
        let settings =
            unsafe { slice::from_raw_parts(self.native_settings, self.native_settings_count) };
        
        if let Some(index) = self.find_setting(id) {
            SStringResult {
                result: SettingsResult::NotFound,
                value: settings[index].string_fixed_value.value,
            }
        } else {
            SStringResult {
                result: SettingsResult::NotFound,
                value: std::ptr::null(),
            }
        }
    }

    pub fn get_int(&mut self, _ext: &str, id: &str) -> SIntResult {
        let settings =
            unsafe { slice::from_raw_parts(self.native_settings, self.native_settings_count) };
        
        if let Some(index) = self.find_setting(id) {
            SIntResult {
                result: SettingsResult::NotFound,
                value: settings[index].int_value.value,
            }
        } else {
            SIntResult {
                result: SettingsResult::NotFound,
                value: 0,
            }
        }
    }

    pub fn get_float(&mut self, _ext: &str, id: &str) -> SFloatResult {
        let settings =
            unsafe { slice::from_raw_parts(self.native_settings, self.native_settings_count) };
        
        if let Some(index) = self.find_setting(id) {
            SFloatResult {
                result: SettingsResult::NotFound,
                value: settings[index].float_value.value,
            }
        } else {
            SFloatResult {
                result: SettingsResult::NotFound,
                value: 0.0,
            }
        }
    }

    pub fn get_bool(&mut self, _ext: &str, _id: &str) -> SBoolResult {
        let settings =
            unsafe { slice::from_raw_parts(self.native_settings, self.native_settings_count) };
        
        if let Some(index) = self.find_setting(id) {
            SBoolResult {
                result: SettingsResult::NotFound,
                value: settings[index].bool_value.value,
            }
        } else {
            SBoolResult {
                result: SettingsResult::NotFound,
                value: false,
            }
        }
    }

    fn find_setting(&self, id: &str) -> Option<usize> {
        let settings =
            unsafe { slice::from_raw_parts(self.native_settings, self.native_settings_count) };
        for (i, s) in settings.iter().enumerate() {
            if s.int_value.s_base.get_widget_id() == id {
                return Some(i);
            }
        }

        None
    }
}
