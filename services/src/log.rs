use crate::LogLevel;
use log::log;
use std::{
    borrow::Cow,
    ffi::CStr,
    os::raw::{c_char, c_void},
    slice, str,
};

extern "C" {
    fn c_log_func(
        self_c: *mut c_void,
        level: u32,
        file: *const c_char,
        line: i32,
        fmt: *const c_char,
        ...
    );
}

#[allow(dead_code)]
#[no_mangle]
unsafe extern "C" fn rust_log_callback(
    user_data: *const c_void,
    level: crate::LogLevel,
    buffer: *const i8,
    buffer_size: i32,
    filename: *const c_char,
    line: i32,
) {
    let log: &Log = { &*(user_data as *const Log) };
    let c_string = {
        let range = slice::from_raw_parts(buffer as _, buffer_size as _);
        str::from_utf8_unchecked(range)
    };

    let filename = if !filename.is_null() {
        let filename_ = { CStr::from_ptr(filename) };
        filename_.to_string_lossy()
    } else {
        Cow::Borrowed("")
    };

    let log_level = match level {
        LogLevel::Trace => log::Level::Trace,
        LogLevel::Debug => log::Level::Debug,
        LogLevel::Info => log::Level::Info,
        LogLevel::Warn => log::Level::Warn,
        LogLevel::Error => log::Level::Error,
        LogLevel::Fatal => log::Level::Error,
    };

    if !filename.is_empty() {
        log!(
            log_level,
            "[{}] {}:{} {}",
            log.instance_name,
            filename,
            line,
            c_string
        );
    } else {
        log!(log_level, "[{}] {}", log.instance_name, c_string);
    }
}

pub struct Log {
    pub instance_name: String,
}

impl Log {
    pub fn new(name: &str) -> Log {
        Log {
            instance_name: name.to_owned(),
        }
    }

    pub fn new_box_leak(name: &str) -> *mut Log {
        Box::leak(Box::new(Log::new(name)))
    }

    pub fn new_c_api(name: &str) -> *const crate::LogFFI {
        let rust_data = Self::new_box_leak(name);
        let c_api = Box::new(crate::LogFFI {
            private_data: rust_data as _,
            log: c_log_func,
        });

        Box::leak(c_api)
    }

    pub fn free_c_api(log: *mut crate::LogFFI) {
        let c_log = unsafe { Box::from_raw(log) };
        let _ = unsafe { Box::from_raw(c_log.private_data as *mut Log) };
    }
}
