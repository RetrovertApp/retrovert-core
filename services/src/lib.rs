pub mod ffi_gen;
pub mod io;
pub mod log;
pub mod metadata;
pub mod settings;
pub use ffi_gen::*;

// It's not safe to pass pointers to other therads, so we use this to get around it
unsafe impl Sync for PluginService {}
unsafe impl Send for PluginService {}

pub struct PluginService {
    service_api: *const ServiceFFI,
}

impl PluginService {
    pub fn new(log_name: &str) -> PluginService {
        let io_api = Box::leak(Box::new(io::Io::new()));
        let metadata_api = Box::leak(Box::new(metadata::Metadata::new()));
        let settings_api = Box::leak(Box::new(settings::Settings::new()));

        let service_api = Box::new(ServiceApi {
            c_io_api: Box::leak(Box::new(IoFFI::new(io_api as _))) as _,
            c_metadata_api: Box::leak(Box::new(MetadataFFI::new(metadata_api as _))) as _,
            c_settings_api: Box::leak(Box::new(SettingsFFI::new(settings_api as _))) as _,
            c_log_api: log::Log::new_c_api(log_name),
        });

        PluginService {
            service_api: Box::leak(Box::new(ServiceFFI::new(Box::leak(service_api) as _))) as _,
        }
    }

    pub fn clone_with_log_name(base: &PluginService, log_name: &str) -> PluginService {
        let base_api_ffi: &mut ServiceFFI = unsafe { &mut *(base.service_api as *mut ServiceFFI) };
        let base_api: &mut ServiceApi =
            unsafe { &mut *(base_api_ffi.private_data as *mut ServiceApi) };

        let service_api = Box::new(ServiceApi {
            c_io_api: base_api.c_io_api,
            c_metadata_api: base_api.c_metadata_api,
            c_settings_api: base_api.c_settings_api,
            c_log_api: log::Log::new_c_api(log_name),
        });

        PluginService {
            service_api: Box::leak(Box::new(ServiceFFI::new(Box::leak(service_api) as _))) as _,
        }
    }

    #[inline]
    pub fn get_c_api(&self) -> *const ServiceFFI {
        self.service_api
    }

    pub fn get_settings_c_api(&self) {

    }



}

impl Drop for PluginService {
    fn drop(&mut self) {
        //
    }
}
