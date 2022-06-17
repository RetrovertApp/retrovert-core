use anyhow::{bail, Context, Result};
use log::{error, trace, LevelFilter, Log, SetLoggerError};
use services::PluginService;
use std::path::{Path, PathBuf};
use std::ffi::CStr;
use vfs::Vfs;
use std::os::raw::c_char;

pub mod output;
pub mod playback;
pub mod plugin_handler;
pub mod playlist;

use plugin_handler::Plugins;
use playlist::Playlist;
use playback::Playback;
use output::Output;


#[derive(Default, Debug)]
pub struct Args {
    /// Directory path for data
    pub data_dir: PathBuf,
    /// Directory path for plugins/and or direct link to plugins. If directory then it will recurse and search
    pub plugin_paths: Vec<String>,
    /// file(s)/path(s) to play
    pub play: Vec<String>,
    /// Randomize the playing
    pub randomize: bool,
}

pub struct Core {
    pub plugin_service: PluginService,
    pub plugins: Plugins,
    pub playlist: Playlist,
    pub vfs: Vfs,
    pub output: Output,
}

impl Core {
    pub fn new(args: &Args) -> Box<Core> {
        // TODO: Fix unwraps
        let mut plugins = Plugins::default();
        let vfs = Vfs::new();
        let plugin_service = PluginService::new("core", vfs.clone());

        dbg!(&args.plugin_paths);

        // Add plugins
        for path in &args.plugin_paths {
            plugins.add_plugins_from_path(path, &plugin_service);
        }

        let playback = Playback::new(plugins.resample_plugins.clone()).unwrap();
        let playlist = Playlist::new(&vfs, &playback, plugins.decoder_plugins.clone()).unwrap();
        let mut output = Output::new(&playback, plugins.output_plugins.clone());

        output.create_default_output();

        Box::new(Core {
            plugin_service,
            plugins,
            vfs,
            playlist,
            output,
        })
    }

    pub fn load_url(&mut self, url: &str) {
        self.playlist.play_url(url);
    }

    pub fn update(&mut self) {
        //self.pla
    }
}

/// Finds the data directory relative to the executable.
/// This is because it's possible to have data next to the exe, but also running
/// the applications as targeht/path/exe and the location is in the root then
fn find_data_directory() -> Result<PathBuf> {
    let current_path = std::env::current_dir().with_context(|| "Unable to get current dir!")?;
    if current_path.join("data").exists() {
        return Ok(current_path);
    }

    let mut path = current_path
        .parent()
        .with_context(||"Unable to get parent dir")?;

    loop {
        trace!("seaching for data in {:?}", path);

        if path.join("data").exists() {
            return Ok(path.to_path_buf());
        }

        path = path.parent().with_context(|| "Unable to get parent dir")?;
    }
}

fn init_data_directory(datadir_over: &Option<String>) -> Result<PathBuf> {
    let current_exe = std::env::current_exe()?;
    std::env::set_current_dir(
        current_exe
            .parent()
            .with_context(|| "Unable to get parent directory")?,
    )?;

    let datadir = if let Some(over_path) = datadir_over {
        let over = Path::new(over_path);

        if !over.exists() {
            bail!(
                "--datadir {} doesn't exist. Is the path incorrect?",
                over_path
            );
        }
        over.to_path_buf()
    } else {
        find_data_directory()?
    };

    // This to enforce we load relative to the current exe
    let current_exe = std::env::current_exe()?;
    std::env::set_current_dir(
        current_exe
            .parent()
            .with_context(|| "Unable to get parent directory")?,
    )?;

    Ok(datadir)
}

fn get_dirs_files(args: &mut pico_args::Arguments, opt: &'static str) -> Result<Vec<String>> {
    let mut output = Vec::new();

    loop {
        let opt_data: Option<String> = args.opt_value_from_str(opt)?;

        if let Some(data) = opt_data {
            output.push(data);
        } else {
            return Ok(output);
        }
    }
}

//
fn init_core_create() -> Result<Args> {
    let mut pargs = pico_args::Arguments::from_env();
    let datadir_over: Option<String> = pargs.opt_value_from_str("--data-dir").unwrap();

    dbg!(&pargs);

    Ok(Args {
        data_dir: init_data_directory(&datadir_over)?,
        plugin_paths: get_dirs_files(&mut pargs, "--plugins")?,
        play: get_dirs_files(&mut pargs, "--play")?,
        randomize: pargs.contains("--randomize"),
    })
}

#[no_mangle]
pub fn core_create() -> *mut Core {
    let args = match init_core_create() {
        Err(e) => {
            error!("Unable to create core: {:?}", e);
            return std::ptr::null_mut();
        }
        Ok(args) => args,
    };

    let core = Box::leak(Core::new(&args));

    trace!("core create");
    core as *mut Core
}

/// # Safety
///
/// Foobar
#[no_mangle]
pub unsafe fn core_destroy(core: *mut Core, _prepare_reload: bool) {
    let _ = Box::from_raw(core);
    trace!("core destroy");
}

/// # Safety
///
/// Foobar
#[no_mangle]
pub unsafe fn core_update(core: *mut Core) {
    let core: &mut Core = &mut *core;
    core.update();
}

/// # Safety
///
/// Foobar
#[no_mangle]
pub unsafe fn core_load_url(core: *mut Core, url: *const c_char) {
    let core: &mut Core = &mut *core;
    let name = CStr::from_ptr(url);
    core.load_url(&name.to_string_lossy());
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
