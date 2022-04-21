use anyhow::{bail, Result};
use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use log::{error, info, trace};
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

#[derive(Default)]
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
        base_service: &PluginService,
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
