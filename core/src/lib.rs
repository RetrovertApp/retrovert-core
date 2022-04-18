use anyhow::Result;
use log::{error, LevelFilter, Log, SetLoggerError};

pub mod plugin_handler;

pub struct Core {
    pub dummy: u32,
}

impl Core {
    pub fn new() -> Core {
        Core { dummy: 0 }
    }

    pub fn update(&mut self) {}
}

#[no_mangle]
pub fn core_create() -> *mut Core {
    let core = Box::leak(Box::new(Core::new()));
    error!("core create");
    core as *mut Core
}

#[no_mangle]
pub fn core_destroy(core: *mut Core, _prepare_reload: bool) {
    let _ = unsafe { Box::from_raw(core) };
    error!("core destroy");
}

#[no_mangle]
pub fn core_update(core: *mut Core) {
    let core: &mut Core = unsafe { &mut *core };
    error!("created update");
    core.update();
}

#[no_mangle]
pub fn core_setup_logger(
    logger: &'static dyn Log,
    level: LevelFilter,
) -> Result<(), SetLoggerError> {
    log::set_max_level(level);
    log::set_logger(logger)
}
