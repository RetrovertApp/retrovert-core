use std::os::raw::c_void;
use std::ptr;

use crate::ffi_gen::IoReadUrlResult;

pub struct Io {
    dummy: u32,
}

impl Io {
    pub fn new() -> Io {
        Io { dummy: 0 }
    }

    pub fn exists(&mut self, _url: &str) -> bool {
        false
    }

    pub fn read_url_to_memory(&mut self, _url: &str) -> IoReadUrlResult {
        IoReadUrlResult {
            data: ptr::null(),
            data_size: 0,
        }
    }

    pub fn free_url_to_memory(&mut self, _data: *const c_void) {}
}
