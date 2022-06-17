use std::os::raw::c_void;
use vfs::{Vfs, RecvMsg};
use std::{thread, ptr, time::Duration};
use log::{error, info};

use crate::ffi_gen::IoReadUrlResult;

pub struct Io {
    vfs: Vfs,
}

impl Io {
    pub fn new(vfs: Vfs) -> Io {
        Io { vfs }
    }

    pub fn exists(&mut self, _url: &str) -> bool {
        false
    }

    pub fn read_url_to_memory(&mut self, url: &str) -> IoReadUrlResult {
        let handle = self.vfs.load_url(url);
        
        for _ in 0..100 {
            match handle.recv.recv_timeout(Duration::from_millis(1)) {
                Ok(RecvMsg::ReadDone(data)) => {
                    return IoReadUrlResult {
                        data: data.ptr, 
                        data_size: data.size as _,
                    }
                },
                Ok(RecvMsg::Error(e)) => {
                    error!("{:?}", e);
                    break;
                },
                Err(e) => {
                    error!("{:?}", e);
                    break;
                }
                _ => (),
            }
        }

        IoReadUrlResult {
            data: ptr::null(),
            data_size: 0,
        }
    }

    pub fn free_url_to_memory(&mut self, _data: *const c_void) {}
}
