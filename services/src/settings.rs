use crate::ffi_gen::*;
use std::ptr;

pub struct Settings {
    _foo: u32,
}

impl Settings {
    pub fn new() -> Settings {
        Settings { _foo: 0 }
    }

    pub fn reg(&mut self, _name: &str, _settings: &[Setting]) -> SettingsResult {
        SettingsResult::Ok
    }

    pub fn get_string(&mut self, _ext: &str, _id: &str) -> SStringResult {
        SStringResult {
            result: SettingsResult::NotFound,
            value: ptr::null(),
        }
    }

    pub fn get_int(&mut self, _ext: &str, _id: &str) -> SIntResult {
        SIntResult {
            result: SettingsResult::NotFound,
            value: 0,
        }
    }

    pub fn get_float(&mut self, _ext: &str, _id: &str) -> SFloatResult {
        SFloatResult {
            result: SettingsResult::NotFound,
            value: 0,
        }
    }

    pub fn get_bool(&mut self, _ext: &str, _id: &str) -> SBoolResult {
        SBoolResult {
            result: SettingsResult::NotFound,
            value: false,
        }
    }
}
