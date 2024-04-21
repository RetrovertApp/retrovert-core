use anyhow::{bail, Result};
use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use log::{error, trace};
use plugin_types::{ProbeResult};
use services::PluginService;
use std::{sync::Arc};
use walkdir::{DirEntry, WalkDir};
use parking_lot::RwLock;

pub struct PlaybackPlugin {
    pub plugin: Library,
    pub service: PluginService,
    pub plugin_path: String,
    pub plugin_funcs: plugin_types::PlaybackPlugin,
}

pub struct OutputPlugin {
    pub plugin: Library,
    pub service: PluginService,
    pub plugin_path: String,
    pub plugin_funcs: plugin_types::OutputPlugin,
}

pub struct ResamplePlugin {
    pub plugin: Library,
    pub service: PluginService,
    pub plugin_path: String,
    pub plugin_funcs: plugin_types::ResamplePlugin,
}

pub type PlaybackPlugins = Arc<RwLock<Vec<Box<PlaybackPlugin>>>>;
pub type OutputPlugins = Arc<RwLock<Vec<Box<OutputPlugin>>>>;
pub type ResamplePlugins = Arc<RwLock<Vec<Box<ResamplePlugin>>>>;

#[derive(Default)]
pub struct Plugins {
    pub decoder_plugins: PlaybackPlugins,
    pub output_plugins: OutputPlugins,
    pub resample_plugins: ResamplePlugins,
}

impl PlaybackPlugin {
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

macro_rules! add_plugin {
    ($plugins:expr, $plugin:expr, $base_service:expr, $name:expr, $type_name:expr, $entry_point:expr,$plugin_type:ident)=>{
        {
        let playback_func: Result<Symbol<extern "C" fn() -> *const plugin_types::$plugin_type>, libloading::Error> =
            unsafe { $plugin.get($entry_point) };

        if let Ok(func) = playback_func {
            let plugin_funcs = unsafe { *func() };

            let plugin_name = plugin_funcs.get_name();
            let version = plugin_funcs.get_version();
            let full_name = format!("{} {}", plugin_name, version);

            trace!("Loaded {} plugin {} {}", $type_name, plugin_name, version);

            let service = PluginService::clone_with_log_name($base_service, &full_name);

            if plugin_funcs.static_init as usize != 0 {
                unsafe {
                    (plugin_funcs.static_init)(service.get_c_api());
                }
            }

            // TODO: Fix unwrap
            let mut p = $plugins.write();

            p.push(Box::new($plugin_type {
                plugin: $plugin,
                service,
                plugin_path: $name.to_owned(),
                plugin_funcs,
            }));

            return Ok(true);
        }
        }
    }
}


impl Plugins {
    pub fn new() -> Plugins {
        Plugins {
            decoder_plugins: Arc::new(RwLock::new(Vec::new())),
            output_plugins: Arc::new(RwLock::new(Vec::new())),
            resample_plugins: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn add_plugin_lib(
        &mut self,
        name: &str,
        plugin: Library,
        base_service: &PluginService,
    ) -> Result<bool> {
        add_plugin!(self.decoder_plugins, plugin, base_service, name, "playback", b"rv_playback_plugin\0", PlaybackPlugin);
        add_plugin!(self.output_plugins, plugin, base_service, name, "output", b"rv_output_plugin\0", OutputPlugin);
        add_plugin!(self.resample_plugins, plugin, base_service, name, "resample", b"rv_resample_plugin\0", ResamplePlugin);
        bail!("No correct entry point found for plugin {}", name)
    }

    fn check_file_type(entry: &DirEntry) -> bool {
        let path = entry.path();

        if let Some(ext) = path.extension() {
            ext == "rvp"
        } else {
            false
        }
    }

    pub fn add_plugins_from_path(&mut self, path: &str, base_service: &PluginService) {
        trace!("Searching path {:} for plugins", path);

        for entry in WalkDir::new(path).into_iter().flatten() {
            if Self::check_file_type(&entry) {
                self.add_plugin(entry.path().to_str().unwrap(), base_service);
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
