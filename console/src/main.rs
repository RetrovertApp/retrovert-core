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

    // Play the queued song(s) to the end, then exit. Loading + decoding starts
    // asynchronously, so wait for playback to begin before watching for its end.
    // ponytail: fixed 5s start timeout; bump it if slow (e.g. network) sources land.
    let start = std::time::Instant::now();
    let mut started = false;
    loop {
        if core.is_playing() {
            started = true;
        } else if started {
            break;
        } else if start.elapsed() > std::time::Duration::from_secs(5) {
            error!("Nothing started playing within 5s; exiting");
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    // Let the buffered tail (up to ~1s, half the ring buffer) drain to the
    // speakers before the cpal stream is dropped.
    std::thread::sleep(std::time::Duration::from_millis(1000));

    Ok(())
}
