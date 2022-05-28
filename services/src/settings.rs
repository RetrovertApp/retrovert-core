use crate::ffi_gen::*;
use anyhow::Result;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::CStr,
    fs::File,
    io::{Read, Write},
    mem,
    os::raw::c_char,
    path::Path,
    ptr, slice,
};
use toml;

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

#[derive(Serialize, Deserialize)]
struct SerPluginTypeSettings {
    plugin_name: String,
    settings: Vec<SerSetting>,
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
    settings: HashMap<String, NativeSettings>,
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

    fn build_ser_settings(&self) -> Vec<SerSetting> {
        debug!("Serializing settings");

        let mut ser_settings = Vec::new();
        let native_settings =
            unsafe { slice::from_raw_parts(self.native_settings, self.native_settings_count) };

        let bytes_size = std::mem::size_of::<Setting>();

        for (s, t) in self.stored_settings.iter().zip(native_settings.iter()) {
            let t0 =
                unsafe { slice::from_raw_parts(mem::transmute::<_, *const u8>(t), bytes_size) };
            let t1 =
                unsafe { slice::from_raw_parts(mem::transmute::<_, *const u8>(s), bytes_size) };

            if t0 != t1 {
                let id = unsafe { s.int_value.s_base.get_widget_id() };
                debug!("field that differs is {}", &id);

                let t = unsafe {
                    match s.int_value.s_base.widget_type as _ {
                        RVS_FLOAT_TYPE => {
                            SerSetting::new(&id, SerValue::FloatValue(s.float_value.value))
                        }
                        RVS_INTEGER_TYPE => {
                            SerSetting::new(&id, SerValue::IntValue(s.int_value.value))
                        }
                        RVS_INTEGER_RANGE_TYPE => {
                            SerSetting::new(&id, SerValue::IntValue(s.int_value.value))
                        }
                        RVS_BOOL_TYPE => {
                            SerSetting::new(&id, SerValue::BoolValue(s.bool_value.value))
                        }
                        RVS_STRING_RANGE_TYPE => {
                            let value = s.string_fixed_value.get_value();
                            SerSetting::new(&id, SerValue::StrValue(value.to_string()))
                        }
                        t => {
                            warn!(
                                "Setting id {} unknown {}",
                                t, s.int_fixed_value.s_base.widget_type
                            );
                            SerSetting::new("", SerValue::NoSetting)
                        }
                    }
                };

                ser_settings.push(t);
            }
        }

        ser_settings
    }

    fn write_internal(&self, path: &str) -> Result<()> {
        let ser_data = self.build_ser_settings();
        let mut file = File::create(&path)?;

        let toml = toml::to_string(&ser_data)?;
        file.write_all(toml.as_bytes())?;

        Ok(())
    }

    fn read_to_file(path: &str) -> Result<String> {
        let mut data = String::new();
        let mut file = File::open(&path)?;
        file.read_to_string(&mut data)?;
        Ok(data)
    }

    fn find_id<'a>(data: &'a mut [Setting], id: &str) -> Option<&'a mut Setting> {
        for s in data {
            let widget_type = unsafe { s.int_value.s_base.get_widget_id() };

            if id == widget_type {
                return Some(s);
            }
        }

        None
    }

    fn get_string_range_value(s: &Setting, name: &str) -> *const c_char {
        let values = unsafe { s.string_fixed_value.get_values() };

        for v in values {
            let sel = unsafe { CStr::from_ptr(v.value) };
            let sel_name = sel.to_string_lossy();

            if sel_name == name {
                return v.value;
            }
        }

        ptr::null()
    }

    fn patch_data(&mut self, input_data: &[SerSetting]) {
        let data = unsafe {
            slice::from_raw_parts_mut(self.native_settings as *mut _, self.native_settings_count)
        };

        for input in input_data {
            if let Some(wd) = Self::find_id(data, &input.id) {
                match input.value {
                    SerValue::FloatValue(v) => wd.float_value.value = v,
                    SerValue::IntValue(v) => wd.int_value.value = v,
                    SerValue::BoolValue(v) => wd.bool_value.value = v,
                    SerValue::StrValue(ref v) => {
                        wd.string_fixed_value.value = Self::get_string_range_value(wd, v)
                    }
                    SerValue::NoSetting => (),
                }
            } else {
                warn!("Id: {} wasn't found in settings", input.id);
            }
        }
    }

    fn load_internal(&mut self, path: &str) -> Result<()> {
        if std::fs::metadata(path).is_err() {
            return Ok(());
        }

        let data = Self::read_to_file(path).unwrap();

        let s: SerPluginTypeSettings = toml::from_str(&data)?;
        self.patch_data(&s.settings);

        Ok(())
    }

    pub fn load(&mut self, path: &Path, filename: &str) -> Result<()> {
        let dir = path.join(filename);
        self.load_internal(&dir.to_string_lossy())?;
        Ok(())
    }

    pub fn write(&self, path: &Path, filename: &str) -> Result<()> {
        let dir = path.join(filename);
        self.write_internal(&dir.to_string_lossy())?;
        Ok(())
    }
}

impl Settings {
    pub fn new() -> Settings {
        Settings {
            settings: HashMap::new(),
        }
    }

    pub fn reg(&mut self, name: &str, settings: &[Setting]) -> SettingsResult {
        if let Some(_ps) = self.settings.get(name) {
            info!("Trying to register settings for {} twice, skipping", name);
            SettingsResult::DuplicatedId
        } else {
            self.settings
                .insert(name.to_owned(), NativeSettings::new(settings));
            SettingsResult::Ok
        }
    }

    fn find_reg_settings(&self, reg_id: &str) -> Option<&[Setting]> {
        if let Some(s) = self.settings.get(reg_id) {
            let settings =
                unsafe { slice::from_raw_parts(s.native_settings, s.native_settings_count) };
            Some(settings)
        } else {
            None
        }
    }

    pub fn get_string(&mut self, reg_id: &str, _ext: &str, id: &str) -> SStringResult {
        let s = match self.find_reg_settings(reg_id) {
            Some(s) => s,
            None => {
                return SStringResult {
                    result: SettingsResult::UnknownId,
                    value: ptr::null(),
                }
            }
        };

        if let Some(setting) = Self::find_setting(id, s) {
            SStringResult {
                result: SettingsResult::NotFound,
                value: unsafe { setting.string_fixed_value.value },
            }
        } else {
            SStringResult {
                result: SettingsResult::NotFound,
                value: ptr::null(),
            }
        }
    }

    pub fn get_int(&mut self, reg_id: &str, _ext: &str, id: &str) -> SIntResult {
        let s = match self.find_reg_settings(reg_id) {
            Some(s) => s,
            None => {
                return SIntResult {
                    result: SettingsResult::UnknownId,
                    value: 0,
                }
            }
        };

        if let Some(setting) = Self::find_setting(id, s) {
            SIntResult {
                result: SettingsResult::NotFound,
                value: unsafe { setting.int_value.value },
            }
        } else {
            SIntResult {
                result: SettingsResult::NotFound,
                value: 0,
            }
        }
    }

    pub fn get_float(&mut self, reg_id: &str, _ext: &str, id: &str) -> SFloatResult {
        let s = match self.find_reg_settings(reg_id) {
            Some(s) => s,
            None => {
                return SFloatResult {
                    result: SettingsResult::UnknownId,
                    value: 0.0,
                }
            }
        };

        if let Some(setting) = Self::find_setting(id, s) {
            SFloatResult {
                result: SettingsResult::NotFound,
                value: unsafe { setting.float_value.value },
            }
        } else {
            SFloatResult {
                result: SettingsResult::NotFound,
                value: 0.0,
            }
        }
    }

    pub fn get_bool(&mut self, reg_id: &str, _ext: &str, id: &str) -> SBoolResult {
        let s = match self.find_reg_settings(reg_id) {
            Some(s) => s,
            None => {
                return SBoolResult {
                    result: SettingsResult::UnknownId,
                    value: false,
                }
            }
        };

        if let Some(setting) = Self::find_setting(id, s) {
            SBoolResult {
                result: SettingsResult::NotFound,
                value: unsafe { setting.bool_value.value },
            }
        } else {
            SBoolResult {
                result: SettingsResult::NotFound,
                value: false,
            }
        }
    }

    fn find_setting<'a>(id: &str, settings: &'a [Setting]) -> Option<&'a Setting> {
        for s in settings {
            if unsafe { s.int_fixed_value.s_base.get_widget_id() } == id {
                return Some(s);
            }
        }

        None
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
