use anyhow::{Context, Result};
use log::{error, trace, LevelFilter, Log, SetLoggerError};

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

/// Finds the data directory relative to the executable.
/// This is because it's possible to have data next to the exe, but also running
/// the applications as targeht/path/exe and the location is in the root then
fn find_data_directory() -> Result<()> {
    let current_path = std::env::current_dir().with_context(|| "Unable to get current dir!")?;
    if current_path.join("data").exists() {
        return Ok(());
    }

    let mut path = current_path
        .parent()
        .with_context(|| format!("Unable to get parent dir"))?;

    loop {
        trace!("seaching for data in {:?}", path);

        if path.join("data").exists() {
            std::env::set_current_dir(path)?;
            return Ok(());
        }

        path = path.parent().with_context(|| "Unable to get parent dir")?;
    }
}

fn init_data_directory() -> Result<()> {
    let current_exe = std::env::current_exe()?;
    std::env::set_current_dir(
        current_exe
            .parent()
            .with_context(|| "Unable to get parent directory")?,
    )?;

    find_data_directory().with_context(|| "Unable to find data directory")?;

    // TODO: We should do better error handling here
    // This to enforce we load relative to the current exe
    let current_exe = std::env::current_exe()?;
    std::env::set_current_dir(
        current_exe
            .parent()
            .with_context(|| "Unable to get parent directory")?,
    )?;

    Ok(())
}

#[no_mangle]
pub fn core_create() -> *mut Core {
    let pargs = pico_args::Arguments::from_env();

    dbg!(&pargs);

    match init_data_directory() {
        Err(e) => {
            error!("Unable to find data directory {:?}", e);
            return std::ptr::null_mut();
        }
        _ => (),
    }

    let core = Box::leak(Box::new(Core::new()));

    trace!("core create");
    core as *mut Core
}

#[no_mangle]
pub fn core_destroy(core: *mut Core, _prepare_reload: bool) {
    let _ = unsafe { Box::from_raw(core) };
    trace!("core destroy");
}

#[no_mangle]
pub fn core_update(core: *mut Core) {
    let core: &mut Core = unsafe { &mut *core };
    trace!("created update");
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

#[no_mangle]
pub extern "C" fn core_show_args() {
    println!("{}", HELP);
}

const HELP: &str = "  --data-dir    PATH    Override data directory. 
  --plugins     PATH    Overide the paths for plugins. Both filenames and directories are supported 
  --play        PATH    Select file(s) to play. Depending on supported sources, urls may be used here as well.
  --randomize           Randomize the files to play if there are more than one.
";
