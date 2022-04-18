use crate::ffi_gen::*;
use std::ptr;

pub struct Settings {
    foo: u32,
}

impl Settings {
    pub fn new() -> Settings {
        Settings { foo: 0 }
    }

    pub fn reg(&mut self, _name: &str, _settings: &[Setting]) -> SettingsResult {
        SettingsResult::Ok
    }

    pub fn get_string(&mut self, ext: &str, id: &str) -> SStringResult {
        SStringResult {
            result: SettingsResult::NotFound,
            value: ptr::null(),
        }
    }

    pub fn get_int(&mut self, ext: &str, id: &str) -> SIntResult {
        SIntResult {
            result: SettingsResult::NotFound,
            value: 0,
        }
    }

    pub fn get_float(&mut self, ext: &str, id: &str) -> SFloatResult {
        SFloatResult {
            result: SettingsResult::NotFound,
            value: 0,
        }
    }

    pub fn get_bool(&mut self, ext: &str, id: &str) -> SBoolResult {
        SBoolResult {
            result: SettingsResult::NotFound,
            value: false,
        }
    }
}
