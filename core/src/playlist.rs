use vfs::Vfs;
use crossbeam_channel::unbounded;
use std::{thread, sync::Mutex};
use log::{error, trace};
use vfs::RecvMsg as VfsRecvMsg;
use std::path::Path;
use std::ptr;
use anyhow::Result;
use cfixed_string::CFixedString;

use crate::plugin_handler::{PlaybackPlugins};
use crate::playback::{Playback, PlaybackMessage, PlaybackPluginInstance};

enum EntryType {
    SubSong(usize),
    /// Plugin required for opening this path
    Driver(String),
}

struct Entry {
    /// Entry type
    entry_type: EntryType,
    /// Used for accelerating data extracton
    extractor_data: Vec<u8>,
    entry_url: String,
}

/*
struct PlaylistEntry {
    entries: Vec<Entries>,
}
*/

pub(crate) enum ActionAfterLoad {
    Play,
    AddUrl,
}

pub(crate) struct VfsHandle {
    /// Original Url that was requested to be loaded 
    pub(crate) url: String,
    /// Handle to check status for the loading/processing on the VFS
    pub(crate) vfs_handle: vfs::Handle,
    // Message to send back to main thread
    pub(crate) ret_msg: crossbeam_channel::Sender<PlaylistReply>,
    // This is the action to take after the load has finished
    pub(crate) action: ActionAfterLoad,
}

impl VfsHandle {
    fn new(url: &str, vfs_handle: vfs::Handle, ret_msg: &crossbeam_channel::Sender<PlaylistReply>, action: ActionAfterLoad) -> VfsHandle {
        VfsHandle {
            url: url.to_owned(),
            vfs_handle,
            ret_msg: ret_msg.clone(),
            action
        }
    }
}

struct PlaylistInternal {
    /// Used for sending messages to the VirtualFileSystem
    vfs: Vfs,
    /// Used for sending messages to the playback (i.e queue new songs)
    playback: Playback,
    /// Handles that are being loaded/processed
    inprogress: Vec<VfsHandle>,
    /// List of plugins that supports playback. We loop over these and figure out if they can play something
    playback_plugins: PlaybackPlugins,
}

// Replies from the Playlist
pub enum PlaylistReply {
    /// When the path isn't found
    NotFound(String),
    /// Path isn't supported
    NotSupported(String),
    /// Path isn't supported
    PlaybackStarted(String),
}

// Messages to send to play list
pub enum PlaylistMessage {
    /// Add url to the playlist 
    AddUrl(String, crossbeam_channel::Sender<PlaylistReply>),
    /// Add url to the playlist and start playing it 
    PlayUrl(String, crossbeam_channel::Sender<PlaylistReply>),
}

pub struct Playlist {
    /// for sending messages to the main-thread
    main_send: crossbeam_channel::Sender<PlaylistMessage>,
}

/// Handle to check state of message sent
pub struct PlaylistHandle {
    pub recv: crossbeam_channel::Receiver<PlaylistReply>,
}


impl PlaylistInternal {
    fn new(vfs: &Vfs, playback: &Playback, playback_plugins: PlaybackPlugins) -> PlaylistInternal {
        PlaylistInternal { 
            vfs: vfs.clone(),
            playback: playback.clone(),
            inprogress: Vec::new(),
            playback_plugins,
        }
    }
}

/// Handles incoming messages (usually from the main thread)
fn incoming_msg(state: &mut PlaylistInternal, msg: &PlaylistMessage) {
    match msg {
        // TODO: Implement
        PlaylistMessage::AddUrl(_path, _ret_msg) => {
            //let vfs_handle = state.vfs.load_url(path);
            //state.inprogress.push(VfsHandle::new(vfs_handle, ret_msg, ActionAfterLoad::AddUrl));
        },

        PlaylistMessage::PlayUrl(url, ret_msg) => {
            trace!("Playlist: adding {} to vfs", url);
            let vfs_handle = state.vfs.load_url(url);
            state.inprogress.push(VfsHandle::new(url, vfs_handle, ret_msg, ActionAfterLoad::Play));
        },
    }
}

/// Given data and a string find a player for it
fn find_playback_plugin(state: &mut PlaylistInternal, url: &str, data: Box<[u8]>) {
    let path = Path::new(url);
    let filename = match path.file_name() {
        None => "".into(),
        Some(name) => name.to_string_lossy(),
    };

    let players = state.playback_plugins.read();

    for player in &*players {
        let plugin_name = player.plugin_funcs.get_name();
        // Checking ifplugin can play this url
        trace!("{} : checking if plugin can can play: {}", plugin_name, url);

        if player.probe_can_play(&data, data.len(), &filename, data.len() as _) {
            trace!("{} : reports that it can play file. Trying to create player instance for playback", plugin_name); 

            let service_funcs = player.service.get_c_api();
            let user_data = unsafe { ((player.plugin_funcs).create)(service_funcs) };

            if user_data.is_null() {
                error!("{} : unable to allocate instance, skipping playback", plugin_name); 
                continue;
            }

            // TODO: Fix settings
            //let c_name = CFixedString::from_str(&filename);
            let open_state = unsafe { ((player.plugin_funcs).open_from_memory)(user_data, data.as_ptr(), data.len() as _, 0, ptr::null()) };

            if open_state < 0 {
                error!("{} : Unable to create playback", plugin_name); 
                unsafe { ((player.plugin_funcs).destroy)(user_data) };
                continue;
            }

            let instance = PlaybackPluginInstance { user_data, plugin: player.plugin_funcs };
            state.playback.channel.send(PlaybackMessage::QueuePlayback(instance)).unwrap();

            return;
        }
    }
}

fn update(state: &mut PlaylistInternal) {
    // Process loading in progress
    let mut i = 0;
    while i < state.inprogress.len() {
        let handle = &state.inprogress[i];

        match handle.vfs_handle.recv.try_recv() {
            Ok(VfsRecvMsg::Error(err)) => {
                error!("Error processing vfs handle {:?}", err);
                state.inprogress.remove(i);
            }

            Ok(VfsRecvMsg::ReadDone(data)) => {
                let name = handle.url.to_owned();
                trace!("Got data back from vfs (size {})", data.len());
                find_playback_plugin(state, &name, data);
                state.inprogress.remove(i);
            }
            /*
            Err(e) => {
                error!("Error processing vfs handle {:?}", e);
                state.inprogress.remove(i);
            }
            */
            _  => (),
        }

        if state.inprogress.len() >= i {
            break;
        }

        i += 1;
    }
}

impl Playlist {
    /// Add url to the playlist
    pub fn add_url(&self, path: &str) -> PlaylistHandle {
        let (thread_send, main_recv) = unbounded::<PlaylistReply>();

        trace!("Playlist: adding {}", path);

        self.main_send
            .send(PlaylistMessage::AddUrl(path.into(), thread_send))
            .unwrap();

        PlaylistHandle { recv: main_recv }
    }

    /// Add url to playlist and play it 
    pub fn play_url(&self, path: &str) -> PlaylistHandle {
        let (thread_send, main_recv) = unbounded::<PlaylistReply>();

        trace!("Playlist: adding for playback {}", path);

        self.main_send
            .send(PlaylistMessage::PlayUrl(path.into(), thread_send))
            .unwrap();

        PlaylistHandle { recv: main_recv }
    }

    pub fn new(vfs: &Vfs, playback: &Playback, playback_plugins: PlaybackPlugins) -> Result<Playlist> {
        let (main_send, thread_recv) = unbounded::<PlaylistMessage>();
                
        let mut state = PlaylistInternal::new(vfs, playback, playback_plugins);

        trace!("Playlist create");

        // Setup worker thread
        thread::Builder::new()
            .name("playlist".to_string())
            .spawn(move || {
                loop {
                    if let Ok(msg) = thread_recv.try_recv() {
                        incoming_msg(&mut state, &msg);
                    } 

                    update(&mut state);
                    // if we didn't get any message we sleep for 1 ms to not hammer the core after one update
                    thread::sleep(std::time::Duration::from_millis(1));
                }
            })?;

        trace!("Playlist create: done");

        Ok(Playlist { main_send })
    }

}
