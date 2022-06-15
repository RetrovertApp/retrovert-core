use vfs::Vfs;
use crossbeam_channel::unbounded;
use std::thread::Thread;
use std::{thread, sync::Mutex};
use log::{error, trace, info};
use vfs::{RecvMsg as VfsRecvMsg, FilesDirs};
use std::path::Path;
use std::ptr;
use anyhow::Result;
use cfixed_string::CFixedString;
use rand::{thread_rng, Rng, rngs::ThreadRng};

use crate::plugin_handler::{PlaybackPlugins};
use crate::playback::{Playback, PlaybackHandle, PlaybackPluginInstance, PlaybackReply};

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


/// Mode of the playlist such as play next song, ranhdomize, etc 
#[derive(PartialEq)]
enum Mode {
    /// Do nothing 
    Default,
    /// Go to the next song in the playlist
    NextSong,
    /// Randomize the playlist 
    Randomize,
}

#[derive(Clone)]
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
    pub(crate) ret_msg: Option<crossbeam_channel::Sender<PlaylistReply>>,
    // This is the action to take after the load has finished
    pub(crate) action: ActionAfterLoad,
}

impl VfsHandle {
    fn new(url: &str, vfs_handle: vfs::Handle, ret_msg: Option<crossbeam_channel::Sender<PlaylistReply>>, action: ActionAfterLoad) -> VfsHandle {
        VfsHandle {
            url: url.to_owned(),
            vfs_handle,
            ret_msg,
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
    /// Handles that are being loaded/processed
    randomize_base_dir: String,
    /// Handles that are being loaded/processed
    randomize_current_dir: String,
    /// Handle to fetch new dir/files
    //randomize_handle: Option<VfsHandle>,
    /// Songs that are currently playing on the decoder thread 
    active_songs: Vec<PlaybackHandle>,
    /// List of plugins that supports playback. We loop over these and figure out if they can play something
    playback_plugins: PlaybackPlugins,
    /// State machine
    mode: Mode,
    /// Count how many times we tried to randomize, but failed (such as empty directories, non-playable files, etc) 
    /// If we reach a certain limit we can tell the user about this and if they want to try to play more 
    /// they can bump the limit
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
            randomize_current_dir: String::new(),
            //randomize_handle: None,
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
            let vfs_handle = state.vfs.load_url(url);
            state.inprogress.push(VfsHandle::new(url, vfs_handle, Some(ret_msg.clone()), ActionAfterLoad::Play));
        },
    }
}

/// Given data and a string find a player for it
fn find_playback_plugin(state: &mut PlaylistInternal, url: &str, data: Box<[u8]>) -> bool {
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
            let playing_track = state.playback.queue_playback(instance).unwrap();

            state.active_songs.push(playing_track);
            
            return true;
        }
    }

    false
}

/// Handles when a directory gets recived as reponse when loading a url
fn update_directory(state: &mut PlaylistInternal, files_dirs: FilesDirs, rng: &mut ThreadRng, progress_index: usize) {
    match state.mode {
        Mode::Randomize => {
            // total number of entries
            let total_len = files_dirs.files.len() + files_dirs.dirs.len(); 
            // if we don't have any files we check if we randomized over n number of tries without finding anything
            // to play. At that point we stop trying and should report it back to the user (currently we just log)
            if total_len == 0 {
                state.missed_randomize_tries += 1;
                // TODO: User configurable
                if state.missed_randomize_tries >= 10 {
                    info!("Tried to randomize {} tries without finding anything playable. Stopping", 10);
                    state.mode = Mode::Default;
                    return;
                }
            }

            state.missed_randomize_tries = 0;
        }

        Mode::NextSong | Mode::Default => (),
    }
}

fn update(state: &mut PlaylistInternal, rng: &mut ThreadRng) {
    // Process loading in progress
    let mut i = 0;

    //let mut new_handles = Vec::new();

    while i < state.inprogress.len() {
        let handle = &state.inprogress[i];

        trace!("state name {} : {}", i, handle.url);

        match handle.vfs_handle.recv.try_recv() {
            Ok(VfsRecvMsg::Error(err)) => {
                error!("Error processing vfs handle {:?}", err);
                state.inprogress.remove(i);
            }

            Ok(VfsRecvMsg::Directory(dir)) => {
                //trace!("{} : {:?}", handle.url, &dir);
                // if we are supposed to randomize do so and grab next
                    // if we don't have any files in this dir we randomize a new one
                let p = if !dir.files.is_empty() {
                    let n = rng.gen_range(0..dir.files.len());
                    Path::new(&handle.url).join(&dir.files[n]).to_owned()
                } else {
                    let n = rng.gen_range(0..dir.dirs.len());
                    Path::new(&handle.url).join(&dir.dirs[n]).to_owned()
                };
                let p = p.to_string_lossy();
                let vfs_handle = state.vfs.load_url(&p);
                state.inprogress[i] = VfsHandle::new(&p, vfs_handle, None, ActionAfterLoad::Play);
            }

            Ok(VfsRecvMsg::ReadDone(data)) => {
                let name = handle.url.to_owned();
                trace!("Got data back from vfs (size {})", data.len());
                find_playback_plugin(state, &name, data);
                state.inprogress.remove(i);
            }
            _  => (),
        }

        if state.inprogress.is_empty() {
            break;
        }

        if state.inprogress[i].url.is_empty() {
            state.inprogress.remove(i);
        }

        i += 1;
    }

    /*/
    for handle in new_handles {
        state.inprogress.push(handle)
    }
    */

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

    if state.active_songs.len() < 2 && state.inprogress.len() <= 2 {
        let vfs_handle = state.vfs.load_url(&state.randomize_base_dir);
        state.state = State::RandomizeNewDir;
        trace!("Push new randomize song {}", state.randomize_base_dir);
        state.inprogress.push(VfsHandle::new(&state.randomize_base_dir, vfs_handle, None, ActionAfterLoad::Play));
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
                rng.gen::<usize>();
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
