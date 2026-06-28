use anyhow::{Context, Result};
use directories::ProjectDirs;
use log::{error, LevelFilter};
use simplelog::{
    ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger, WriteLogger,
};
use std::fs::File;

fn init_logging() -> Result<()> {
    let dirs = ProjectDirs::from("app", "tbl", "retrovert")
        .context("Unable to get a user directory for config and log output")?;

    std::fs::create_dir_all(dirs.config_dir())
        .with_context(|| format!("Unable to create config dir {:?}", dirs.config_dir()))?;

    let log_file_path = dirs.config_dir().join("retrovert.log");
    let log_file = File::create(&log_file_path)
        .with_context(|| format!("Unable to create log file {:?}", log_file_path))?;

    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(LevelFilter::Trace, Config::default(), log_file),
    ])?;

    Ok(())
}

fn main() -> Result<()> {
    init_logging()?;

    let mut pargs = pico_args::Arguments::from_env();
    if pargs.contains(["-h", "--help"]) {
        rv_core::print_help();
        return Ok(());
    }

    let args = rv_core::parse_args()?;
    let mut core = match rv_core::Core::new(&args) {
        Ok(core) => core,
        Err(e) => {
            error!("Unable to create core: {}", e);
            rv_core::print_help();
            return Err(e);
        }
    };

    for url in &args.play {
        core.load_url(url);
    }

    loop {
        core.update();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
