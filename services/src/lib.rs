pub mod ffi_gen;
pub mod io;
pub mod log;
pub mod metadata;
pub mod settings;
pub use ffi_gen::*;

pub struct PluginService {
    service_api: *const ServiceFFI,
}

impl PluginService {
    pub fn new(plugin_name: &str) -> PluginService {
        let io_api = Box::leak(Box::new(io::Io::new()));
        let metadata_api = Box::leak(Box::new(metadata::Metadata::new()));
        let settings_api = Box::leak(Box::new(settings::Settings::new()));

        let service_api = Box::new(ServiceApi {
            c_io_api: Box::leak(Box::new(IoFFI::new(io_api as _))) as _,
            c_metadata_api: Box::leak(Box::new(MetadataFFI::new(metadata_api as _))) as _,
            c_settings_api: Box::leak(Box::new(SettingsFFI::new(settings_api as _))) as _,
            c_log_api: log::Log::new_c_api(plugin_name),
        });

        PluginService {
            service_api: Box::leak(Box::new(ServiceFFI::new(Box::leak(service_api) as _))) as _,
        }
    }

    #[inline]
    pub fn get_c_api(&self) -> *const ServiceFFI {
        self.service_api
    }
}

impl Drop for PluginService {
    fn drop(&mut self) {
        //
    }
}
