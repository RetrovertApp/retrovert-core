use crate::ffi_gen::*;
use log::{debug, info};
use serde;
use std::collections::{HashMap, HashSet};
use std::{mem, ptr, rc::Rc, slice};

#[derive(Clone, Serialize, Deserialize)]
enum SerValue {
    NoSetting,
    FloatValue(f32),
    IntValue(i32),
    BoolValue(bool),
    StrValue(String),
}

#[derive(Serialize, Deserialize)]
struct SerSetting {
    id: String,
    value: SerValue,
}

pub struct PluginSettings {
    settings: HashMap<String, Settings>,
}

impl SerSetting {
    fn new(id: &str, value: SerValue) -> SerSetting {
        SerSetting {
            id: id.to_owned(),
            value,
        }
    }
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

    fn build_ser_settings(settings: &Settings) -> Vec<SerSetting> {
        debug!("Serializing settings");

        let mut ser_settings = Vec::new();
        let native_settings = unsafe {
            slice::from_raw_parts(
                settings.native_settings as *const _,
                settings.native_settings_count,
            )
        };

        let bytes_size = std::mem::size_of::<Setting>();

        for (s, t) in settings.stored_settings.iter().zip(native_settings.iter()) {
            let t0 =
                unsafe { slice::from_raw_parts(mem::transmute::<_, *const u8>(t), bytes_size) };
            let t1 =
                unsafe { slice::from_raw_parts(mem::transmute::<_, *const u8>(s), bytes_size) };

            if t0 != t1 {
                let id = s.int_value.s_base.get_widget_id();
                debug!("field that differs is {}", &id);

                let t = unsafe {
                    match s.int_value.s_base.widget_type as u32 {
                        HS_FLOAT_TYPE => {
                            SerSetting::new(&id, SerValue::FloatValue(s.float_value.value))
                        }
                        HS_INTEGER_TYPE => {
                            SerSetting::new(&id, SerValue::IntValue(s.int_value.value))
                        }
                        HS_INTEGER_RANGE_TYPE => {
                            SerSetting::new(&id, SerValue::IntValue(s.int_value.value))
                        }
                        HS_BOOL_TYPE => {
                            SerSetting::new(&id, SerValue::BoolValue(s.bool_value.value))
                        }
                        HS_STRING_RANGE_TYPE => {
                            let value_us = CStr::from_ptr(s.string_fixed_value.value);
                            let value = value_us.to_string_lossy().to_string();
                            SerSetting::new(&id, SerValue::StrValue(value))
                        }
                        t => {
                            warn!("Setting id {} unknown {}", t, widget_type);
                            SerSetting::new("", SerValue::NoSetting)
                        }
                    }
                };

                ser_settings.push(t);
            }
        }

        ser_settings
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
