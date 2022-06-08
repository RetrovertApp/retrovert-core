use plugin_types::ResamplePlugin;
use anyhow::{Result, bail};

#[derive(Clone)]
pub struct ResamplePluginInstance {
    pub user_data: *mut c_void,
    pub plugin: ResamplePlugin,
}

/*
impl ResamplePluginInstance {
    pub fn create_default_instance(plugins: ResamplePlugins) -> Result<ResamplePluginInstance> {
        let resample_plugins = resample_plugis.read();

        if resample_plugins.is_empty() {
            bail!("No resample plugin(s) found. Unable to setup Retrovert playback");
        }

        Self::create_instance(&plugins[0])
    }

    pub fn create_instance(plugin: &ResamplePlugin) -> Result<ResamplePluginInstance> {
        let plugin_name = op.plugin_funcs.get_name();
        let service_funcs = op.service.get_c_api();
        let user_data = unsafe { ((op.plugin_funcs).create)(service_funcs) };

        if user_data.is_null() {
            bail!("{} : unable to allocate instance", plugin_name);
        }

        trace!("Created default resample plugin: {}", plugin_name);

        Ok(ResamplePluginInstance { user_data, plugin: op.plugin_funcs })
    }
} 
*/
