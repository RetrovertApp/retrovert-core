use anyhow::{bail, Result};
use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use log::{error, trace};
use plugin_types::{PlaybackPlugin, ProbeResult};
use std::{sync::Arc, time::Duration};
use walkdir::{DirEntry, WalkDir};

pub struct DecoderPlugin {
    pub plugin: Library,
    pub plugin_path: String,
    pub plugin_funcs: PlaybackPlugin,
}

pub struct Plugins {
    pub decoder_plugins: Vec<Box<DecoderPlugin>>,
}

impl DecoderPlugin {
    pub fn probe_can_play(
        &self,
        data: &[u8],
        buffer_len: usize,
        filename: &str,
        file_size: u64,
    ) -> bool {
        let c_filename = CFixedString::from_str(filename);
        let res = unsafe {
            ((self.plugin_funcs).probe_can_play)(
                data.as_ptr(),
                buffer_len as _,
                c_filename.as_ptr(),
                file_size,
            )
        };

        match res {
            ProbeResult::Supported => true,
            ProbeResult::Unsupported => false,
            ProbeResult::Unsure => false,
        }
    }

    /*
    pub fn get_metadata(&self, filename: &str, service: &PluginService) {
        let c_filename = CFixedString::from_str(filename);
        unsafe {
            if !self.plugin_funcs.metadata.is_null() {
                let _ =
                    (self.plugin_funcs.metadata)(c_filename.as_ptr(), service.get_c_service_api());
            }
        };
    }
    */
}

#[cfg(target_os = "macos")]
pub fn get_plugin_ext() -> &'static str {
    "dylib"
}

#[cfg(target_os = "linux")]
pub fn get_plugin_ext() -> &'static str {
    "so"
}

#[cfg(target_os = "windows")]
pub fn get_plugin_ext() -> &'static str {
    "dll"
}

impl Plugins {
    pub fn new() -> Plugins {
        Plugins {
            decoder_plugins: Vec::new(),
        }
    }

    fn add_plugin_lib(
        &mut self,
        name: &str,
        plugin: Library,
        service_api: *const services::ServiceFFI,
    ) -> Result<bool> {
        let func: Symbol<extern "C" fn() -> *const plugin_types::PlaybackPlugin> =
            unsafe { plugin.get(b"rv_playback_plugin\0")? };

        let plugin_funcs = unsafe { *func() };

        if plugin_funcs.probe_can_play as u64 == 0 {
            bail!(
                "Unable to add {} due to \"probe_can_play\" function missing",
                name
            );
        }

        if plugin_funcs.supported_extensions as u64 == 0 {
            bail!(
                "Unable to add {} due to \"supported_extensions\" function missing",
                name
            );
        }

        if plugin_funcs.create as u64 == 0 {
            bail!("Unable to add {} due to \"create\" function missing", name);
        }

        if plugin_funcs.destroy as u64 == 0 {
            bail!("Unable to add {} due to \"destroy\" function missing", name);
        }

        if plugin_funcs.read_data as u64 == 0 {
            bail!(
                "Unable to add {} due to \"read_data\" function missing",
                name
            );
        }

        if plugin_funcs.open as u64 == 0 {
            bail!("Unable to add {} due to \"open\" function missing", name);
        }

        if plugin_funcs.close as u64 == 0 {
            bail!("Unable to add {} due to \"close\" function missing", name);
        }

        trace!(
            "Loaded playback plugin {} {}",
            plugin_funcs.get_name(),
            plugin_funcs.get_version()
        );

        /*
        if let Some(static_init) = native_plugin.static_init {
            // TODO: Memory leak
            let name = format!("{} {}", plugin_funcs.name, plugin_funcs.version);
            let c_name = CString::new(name).unwrap();
            let log_api = Box::into_raw(ServiceApi::create_log_api());

            unsafe {
                (*log_api).log_set_base_name.unwrap()((*log_api).priv_data, c_name.as_ptr());
                (static_init)(log_api, service_api);
            }
        }
        */

        self.decoder_plugins.push(Box::new(DecoderPlugin {
            plugin,
            plugin_path: name.to_owned(),
            plugin_funcs,
        }));

        Ok(true)
    }

    fn check_file_type(entry: &DirEntry) -> bool {
        let path = entry.path();

        if let Some(ext) = path.extension() {
            ext == get_plugin_ext()
        } else {
            false
        }
    }

    fn internal_add_plugins_from_path(
        &mut self,
        path: &str,
        service_api: *const services::ServiceFFI,
    ) {
        for entry in WalkDir::new(path).max_depth(1) {
            if let Ok(t) = entry {
                if Self::check_file_type(&t) {
                    self.add_plugin(t.path().to_str().unwrap(), service_api);
                    //println!("{}", t.path().display());
                }
            }
        }
    }

    pub fn add_plugins_from_path(&mut self, service_api: *const services::ServiceFFI) {
        self.internal_add_plugins_from_path("plugins", service_api);
        self.internal_add_plugins_from_path(".", service_api);
    }

    pub fn add_plugin(&mut self, name: &str, service_api: *const services::ServiceFFI) {
        match unsafe { Library::new(name) } {
            Ok(lib) => {
                if let Err(e) = self.add_plugin_lib(name, lib, service_api) {
                    error!("Unable to add {} because of error {:?}", name, e);
                }
            }
            Err(e) => {
                error!("Unable to load dynamic lib, err {:?}", e);
            }
        }
    }
}
