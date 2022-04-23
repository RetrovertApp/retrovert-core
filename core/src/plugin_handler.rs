use anyhow::{bail, Result};
use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use log::{error, trace};
use plugin_types::{PlaybackPlugin, ProbeResult};
use services::PluginService;
//use std::{sync::Arc, time::Duration};
use walkdir::{DirEntry, WalkDir};

pub struct DecoderPlugin {
    pub plugin: Library,
    pub service: PluginService,
    pub plugin_path: String,
    pub plugin_funcs: PlaybackPlugin,
}

pub struct OutputPlugin {
    pub plugin: Library,
    pub service: PluginService,
    pub plugin_path: String,
    pub plugin_funcs: plugin_types::OutputPlugin,
}

#[derive(Default)]
pub struct Plugins {
    pub decoder_plugins: Vec<Box<DecoderPlugin>>,
    pub output_plugins: Vec<Box<OutputPlugin>>,
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

#[allow(dead_code)]
pub type PlaybackReturnStruct = extern "C" fn() -> *const plugin_types::PlaybackPlugin;

#[allow(dead_code)]
pub type OutputReturnStruct = extern "C" fn() -> *const plugin_types::OutputPlugin;

impl Plugins {
    pub fn new() -> Plugins {
        Plugins {
            decoder_plugins: Vec::new(),
            output_plugins: Vec::new(),
        }
    }

    fn add_plugin_lib(
        &mut self,
        name: &str,
        plugin: Library,
        base_service: &PluginService,
    ) -> Result<bool> {
        let playback_func: Result<Symbol<PlaybackReturnStruct>, libloading::Error> =
            unsafe { plugin.get(b"rv_playback_plugin\0") };

        if let Ok(func) = playback_func {
            let plugin_funcs = unsafe { *func() };

            let plugin_name = plugin_funcs.get_name();
            let version = plugin_funcs.get_version();
            let full_name = format!("{} {}", plugin_name, version);

            trace!("Loaded playback plugin {} {}", plugin_name, version);

            let service = PluginService::clone_with_log_name(base_service, &full_name);

            if plugin_funcs.static_init as u64 != 0 {
                unsafe {
                    (plugin_funcs.static_init)(service.get_c_api());
                }
            }

            self.decoder_plugins.push(Box::new(DecoderPlugin {
                plugin,
                service,
                plugin_path: name.to_owned(),
                plugin_funcs,
            }));

            return Ok(true);

            // return self.add_playback_plugin(name, plugin, func, base_service);
        }

        let output_func: Result<Symbol<OutputReturnStruct>, libloading::Error> =
            unsafe { plugin.get(b"rv_output_plugin\0") };

        if let Ok(func) = output_func {
            let plugin_funcs = unsafe { *func() };

            let plugin_name = plugin_funcs.get_name();
            let version = plugin_funcs.get_version();
            let full_name = format!("{} {}", plugin_name, version);

            trace!("Loaded output plugin {} {}", plugin_name, version);

            let service = PluginService::clone_with_log_name(base_service, &full_name);

            if plugin_funcs.static_init as u64 != 0 {
                unsafe {
                    (plugin_funcs.static_init)(service.get_c_api());
                }
            }

            if plugin_funcs.create as u64 != 0 {
                unsafe {
                    (plugin_funcs.create)(service.get_c_api());
                }
            }

            self.output_plugins.push(Box::new(OutputPlugin {
                plugin,
                service,
                plugin_path: name.to_owned(),
                plugin_funcs,
            }));

            return Ok(true);
        }

        bail!("No correct entry point found for plugin {}", name)
    }

    fn check_file_type(entry: &DirEntry) -> bool {
        let path = entry.path();

        if let Some(ext) = path.extension() {
            ext == get_plugin_ext()
        } else {
            false
        }
    }

    pub fn add_plugins_from_path(&mut self, path: &str, base_service: &PluginService) {
        trace!("Searching path {:} for plugins", path);
        for entry in WalkDir::new(path) {
            if let Ok(t) = entry {
                if Self::check_file_type(&t) {
                    self.add_plugin(t.path().to_str().unwrap(), base_service);
                }
            }
        }
    }

    pub fn add_plugin(&mut self, name: &str, base_service: &PluginService) {
        match unsafe { Library::new(name) } {
            Ok(lib) => {
                if let Err(e) = self.add_plugin_lib(name, lib, base_service) {
                    error!("Unable to add {} because of error {:?}", name, e);
                }
            }
            Err(e) => {
                error!("Unable to load dynamic lib, err {:?}", e);
            }
        }
    }
}
