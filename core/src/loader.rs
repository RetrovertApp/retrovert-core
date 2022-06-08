use anyhow::Result;
use crossbeam_channel::unbounded;
use std::{sync::Mutex, thread};
use plugin_handler::OutputPlugins;

use crate::plugin_handler;

pub enum RecvMsg {
    ReadProgress(f32),
    Error(VfsError),
    Directory(FilesDirs),
    NotFound,
}

pub enum SendMsg {
    LoadUrl(String, crossbeam_channel::Sender<RecvMsg>),
}

pub struct Loader {
    /// for sending messages to the main-thread
    main_send: crossbeam_channel::Sender<SendMsg>,
}

pub struct Handle {
    pub recv: crossbeam_channel::Receiver<RecvMsg>,
}

struct LoaderInternal {
    playback_plugins: OutputPlugins,
}

impl Loader {
    //pub fn new(vfs_drivers: Option<&[Box<dyn VfsDriver>]>) -> Vfs {
    pub fn new(plugins: &OutputPlugins) -> Result<Loader> {
        let (main_send, thread_recv) = unbounded::<SendMsg>();
        let loader_internal = LoaderInternal {
            playback_plugins: plugins.clone(),
        };

        // Setup worker thread
        thread::Builder::new()
            .name("loader".to_string())
            .spawn(move || {
                /*
                */
            })?;

        Ok(Loader { main_send })
    }

    pub fn load_url(&self, path: &str) -> Handle {
        let (thread_send, main_recv) = unbounded::<RecvMsg>();

        self.main_send
            .send(SendMsg::LoadUrl(path.into(), thread_send))
            .unwrap();

        Handle { recv: main_recv }
    }
}
