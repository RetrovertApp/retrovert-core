use vfs::Vfs;
use crossbeam_channel::unbounded;
use std::{thread};
use log::{error, trace, info};
use vfs::{RecvMsg as VfsRecvMsg, FilesDirs};
use std::path::Path;
use cfixed_string::CFixedString;
use anyhow::Result;
use rand::{thread_rng, Rng, rngs::ThreadRng};

use crate::plugin_handler::{PlaybackPlugins};
use crate::playback::{Playback, PlaybackHandle, PlaybackPluginInstance, PlaybackReply};

/// Mode of the playlist such as play next song, ranhdomize, etc 
#[derive(PartialEq)]
enum Mode {
    /// Do nothing 
    Default,
    /// Go to the next song in the playlist
    //NextSong,
    /// Randomize the playlist 
    Randomize,
}
pub(crate) struct VfsHandle {
    /// Original Url that was requested to be loaded 
    pub(crate) url: String,
    /// Handle to check status for the loading/processing on the VFS
    pub(crate) vfs_handle: vfs::Handle,
    // Message to send back to main thread
    //pub(crate) ret_msg: Option<crossbeam_channel::Sender<PlaylistReply>>,
}

impl VfsHandle {
    fn new(url: &str, vfs: &Vfs, _ret_msg: Option<crossbeam_channel::Sender<PlaylistReply>>) -> VfsHandle {
        VfsHandle {
            url: url.to_owned(),
            vfs_handle: vfs.load_url(url),
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
    /// Handles that are being loaded/processed
    randomize_base_dir: String,
    /// Songs that are currently playing on the decoder thread 
    active_songs: Vec<PlaybackHandle>,
    /// List of plugins that supports playback. We loop over these and figure out if they can play something
    playback_plugins: PlaybackPlugins,
    /// State machine
    mode: Mode,
    /// If we reach a certain limit we can tell the user about this and if they want to try to play more  they can bump the limit
    missed_randomize_tries: usize,
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
            active_songs: Vec::new(),
            randomize_base_dir: String::new(),
            mode: Mode::Default,
            playback_plugins,
            missed_randomize_tries: 0,
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
            state.mode = Mode::Randomize;
            state.randomize_base_dir = url.to_owned();
            trace!("Playlist: adding {} to vfs", url);
            state.inprogress.push(VfsHandle::new(url, &state.vfs, Some(ret_msg.clone())));
            state.inprogress.push(VfsHandle::new(url, &state.vfs, Some(ret_msg.clone())));
        }
    }
}

/// Given data and a string find a player for it
fn find_playback_plugin(state: &mut PlaylistInternal, url: &str, data: &[u8], progress_index: usize) -> bool {
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

        if player.probe_can_play(data, data.len(), &filename, data.len() as _) {
            trace!("{} : reports that it can play file. Trying to create player instance for playback", plugin_name); 

            let service_funcs = player.service.get_c_api();
            let user_data = unsafe { ((player.plugin_funcs).create)(service_funcs) };

            if user_data.is_null() {
                error!("{} : unable to allocate instance, skipping playback", plugin_name); 
                continue;
            }

            // TODO: Fix settings
            let c_name = CFixedString::from_str(&url);
            //let open_state = unsafe { ((player.plugin_funcs).open_from_memory)(user_data, data.as_ptr(), data.len() as _, 0, ptr::null()) };
            let open_state = unsafe { ((player.plugin_funcs).open)(user_data, c_name.as_ptr(), 0, service_funcs) };

            if open_state < 0 {
                error!("{} : Unable to create playback", plugin_name); 
                unsafe { ((player.plugin_funcs).destroy)(user_data) };
                continue;
            }

            info!("Queueing playback: {}", &state.inprogress[progress_index].url);

            let instance = PlaybackPluginInstance { user_data, plugin: player.plugin_funcs };
            let playing_track = state.playback.queue_playback(instance).unwrap();

            state.active_songs.push(playing_track);
            
            return true;
        }
    }

    false
}

fn get_next_song(state: &mut PlaylistInternal, prev_index: Option<usize>) {
    if state.mode == Mode::Randomize {
        if state.randomize_base_dir.is_empty() {
            return;
        }

        // randomize from base dir
        if let Some(prev_index) = prev_index {
            state.inprogress[prev_index] = VfsHandle::new(&state.randomize_base_dir, &state.vfs, None);
        } else {
            info!("Pushing {} to load", state.randomize_base_dir);
            state.inprogress.push(VfsHandle::new(&state.randomize_base_dir, &state.vfs, None));
        }
    }
}

/// Handles when a directory gets recived as reponse when loading a url
fn update_get_directory(state: &mut PlaylistInternal, files_dirs: FilesDirs, rng: &mut ThreadRng, progress_index: usize) {
    match state.mode {
        Mode::Randomize => {
            let dirs_len = files_dirs.dirs.len();
            let files_len = files_dirs.files.len(); 
            let total_len = dirs_len + files_len; 
            // if we don't have any files we check if we randomized over n number of tries without finding anything
            // to play. At that point we stop trying and should report it back to the user (currently we just log)
            if total_len == 0 {
                state.missed_randomize_tries += 1;
                // TODO: User configurable
                if state.missed_randomize_tries >= 10 {
                    info!("Tried to randomize {} tries without finding anything playable. Stopping", 10);
                    state.mode = Mode::Default;
                    state.inprogress.swap_remove(progress_index);
                    return;
                }

                // if we couldn't find anything in the current directory we re-randomize from the base path
                // TODO: Fix ret message
                state.inprogress[progress_index] = VfsHandle::new(&state.randomize_base_dir, &state.vfs, None);
                return;
            }

            let entry = rng.gen_range(0..total_len);
            let url = if entry < dirs_len {
                &files_dirs.dirs[entry]
            } else {
                &files_dirs.files[entry - dirs_len]
            };

            let path = Path::new(&state.inprogress[progress_index].url).join(url);
            let p = path.to_string_lossy();
            state.inprogress[progress_index] = VfsHandle::new(&p, &state.vfs, None);

            state.missed_randomize_tries = 0;
        }

        Mode::Default => (),
    }
}

fn update_get_read_done(state: &mut PlaylistInternal, data: &[u8], progress_index: usize) {
    trace!("Got data back from vfs (size {})", data.len());
    let url = state.inprogress[progress_index].url.to_owned();
    // if we managed to find a player for the file we will remove it, otherwise if get a text song
    if find_playback_plugin(state, &url, data, progress_index) {
        state.inprogress.remove(progress_index);
    } else {
        trace!("Unable to find player for {} trying to find next song", &url);
        get_next_song(state, Some(progress_index));
    }
}

fn update(state: &mut PlaylistInternal, rng: &mut ThreadRng) {
    // Process loading in progress
    let mut i = 0;

    //let mut new_handles = Vec::new();

    while i < state.inprogress.len() {
        let handle = &state.inprogress[i];

        //trace!("state name {} : {}", i, handle.url);

        match handle.vfs_handle.recv.try_recv() {
            Ok(VfsRecvMsg::Error(err)) => {
                error!("Error processing vfs handle {:?}", err);
                state.inprogress.remove(i);
            }

            Ok(VfsRecvMsg::Directory(dir)) => update_get_directory(state, dir, rng, i),
            Ok(VfsRecvMsg::ReadDone(data)) => update_get_read_done(state, data.get(), i),
            _  => (),
        }

        if i < state.inprogress.len() && state.inprogress[i].url.is_empty() {
            state.inprogress.remove(i);
        }

        i += 1;
    }

    // Process active playing tunes

    while i < state.active_songs.len() {
        let handle = &state.active_songs[i];

        match handle.channel.try_recv() {
            Ok(PlaybackReply::PlaybackStarted) => {
                trace!("Playback started");
            }
            Ok(PlaybackReply::PlaybackEnded) => {
                trace!("Playback ended");
                state.active_songs.remove(i);
            }
            _ => (),
        }

        i += 1;
    }

    //trace!("active songs {} inprogress {}", state.active_songs.len(), state.inprogress.len());

    if state.active_songs.len() == 1 && state.inprogress.is_empty() {
        get_next_song(state, None);
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
                let mut rng = thread_rng();

                loop {
                    if let Ok(msg) = thread_recv.try_recv() {
                        incoming_msg(&mut state, &msg);
                    } 

                    update(&mut state, &mut rng);
                    // if we didn't get any message we sleep for 1 ms to not hammer the core after one update
                    thread::sleep(std::time::Duration::from_millis(1));
                }
            })?;

        trace!("Playlist create: done");

        Ok(Playlist { main_send })
    }

}
