// dlopen each no-visualization cohort plugin and prove the caps = 0 contract:
// the api version is current and every viz vtable slot (get_structure through
// get_vu) is NULL, so the host can only read caps = 0. Where a test module
// exists, also prove the decoder still opens and produces audio (no regression).
// Skips (does not fail) a plugin whose .so is not built.

use cfixed_string::CFixedString;
use libloading::{Library, Symbol};
use plugin_types::{
    AudioFormat, AudioStreamFormat, PlaybackPlugin, ReadData, ReadInfo, ReadStatus, RVService,
};
use services::PluginService;
use std::path::PathBuf;
use vfs::Vfs;

struct NoViz {
    name: &'static str,
    so_env: &'static str,
    so_rel: &'static str,
    module_env: &'static str,
    module_rel: Option<&'static str>,
}

const COHORT: &[NoViz] = &[
    NoViz {
        name: "asap",
        so_env: "RV_ASAP_SO",
        so_rel: "../../playback-asap/build/plugins/asap_playback.so",
        module_env: "RV_ASAP_MODULE",
        module_rel: Some("../../../../replay_frontend/data/test_data/music/asap/a_team.sap"),
    },
    NoViz {
        name: "cpsycle",
        so_env: "RV_CPSYCLE_SO",
        so_rel: "../../playback-cpsycle/build/plugins/cpsycle_playback.so",
        module_env: "RV_CPSYCLE_MODULE",
        module_rel: Some("../../../../replay_frontend/data/test_data/music/cpsycle/test.psy"),
    },
    NoViz {
        name: "gme",
        so_env: "RV_GME_SO",
        so_rel: "../../playback-gme/build/plugins/gme_playback.so",
        module_env: "RV_GME_MODULE",
        module_rel: Some("../../../../replay_frontend/data/test_data/music/gme/super_turrican.nsf"),
    },
    NoViz {
        name: "eupmini",
        so_env: "RV_EUPMINI_SO",
        so_rel: "../../playback-eupmini/build/plugins/eupmini_playback.so",
        module_env: "RV_EUPMINI_MODULE",
        module_rel: None,
    },
    NoViz {
        name: "fmplayer",
        so_env: "RV_FMPLAYER_SO",
        so_rel: "../../playback-fmplayer/build/plugins/fmplayer_playback.so",
        module_env: "RV_FMPLAYER_MODULE",
        module_rel: None,
    },
    NoViz {
        name: "libkss",
        so_env: "RV_LIBKSS_SO",
        so_rel: "../../playback-libkss/build/plugins/libkss_playback.so",
        module_env: "RV_LIBKSS_MODULE",
        module_rel: None,
    },
    NoViz {
        name: "sunvox",
        so_env: "RV_SUNVOX_SO",
        so_rel: "../../playback-sunvox/build/plugins/sunvox_playback.so",
        module_env: "RV_SUNVOX_MODULE",
        module_rel: None,
    },
];

fn resolve(env: &str, rel: &str) -> PathBuf {
    if let Ok(p) = std::env::var(env) {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

// Returns true if any read reported a non-Error status (i.e. the decoder produced
// audio rather than faulting). The vtable edit only touched viz slots, so this is a
// regression smoke test on the still-shared decode path.
fn decode_ok(plugin: &PlaybackPlugin, user_data: *mut std::ffi::c_void) -> bool {
    let mut buf = vec![0f32; 1024 * 2];
    let mut saw_audio = false;
    for _ in 0..8 {
        let rd = ReadData {
            channels_output: buf.as_mut_ptr() as _,
            channels_output_max_bytes_size: (buf.len() * 4) as u32,
            info: ReadInfo {
                format: AudioFormat {
                    audio_format: AudioStreamFormat::S16,
                    channel_count: 2,
                    sample_rate: 48000,
                },
                frame_count: 0,
                status: ReadStatus::DecodingRequest,
            },
        };
        let info = (plugin.read_data.unwrap())(user_data, rd);
        assert_ne!(info.status, ReadStatus::Error, "decode returned Error status");
        if info.frame_count > 0 {
            saw_audio = true;
        }
    }
    saw_audio
}

#[test]
fn no_viz_cohort_caps_zero() {
    let mut checked = 0;
    for p in COHORT {
        let so = resolve(p.so_env, p.so_rel);
        if !so.exists() {
            eprintln!("skipping {}: {} not built", p.name, so.display());
            continue;
        }
        checked += 1;

        let lib = unsafe { Library::new(&so) }.expect("dlopen");
        let entry: Symbol<extern "C" fn() -> *const PlaybackPlugin> =
            unsafe { lib.get(b"rv_playback_plugin\0") }.expect("rv_playback_plugin");
        let plugin = unsafe { &*entry() };

        assert_eq!(
            plugin.api_version,
            plugin_types::RV_PLAYBACK_PLUGIN_API_VERSION,
            "{}: api version drift",
            p.name
        );

        // caps = 0 contract: there is no way to advertise any capability — every
        // viz vtable slot is NULL.
        assert!(plugin.get_structure.is_none(), "{}: get_structure must be NULL", p.name);
        assert!(plugin.get_columns.is_none(), "{}: get_columns must be NULL", p.name);
        assert!(plugin.get_pattern_channels.is_none(), "{}: get_pattern_channels must be NULL", p.name);
        assert!(plugin.get_scope_channels.is_none(), "{}: get_scope_channels must be NULL", p.name);
        assert!(plugin.get_position.is_none(), "{}: get_position must be NULL", p.name);
        assert!(plugin.get_channel_rows.is_none(), "{}: get_channel_rows must be NULL", p.name);
        assert!(plugin.get_cells.is_none(), "{}: get_cells must be NULL", p.name);
        assert!(plugin.set_scope_enabled.is_none(), "{}: set_scope_enabled must be NULL", p.name);
        assert!(plugin.get_scope_samples.is_none(), "{}: get_scope_samples must be NULL", p.name);
        assert!(plugin.get_vu.is_none(), "{}: get_vu must be NULL", p.name);

        // Lifecycle: create/destroy (+ the repositioned static_init/static_destroy
        // slots) must work on the migrated vtable even without a module — this is the
        // only coverage of sunvox's real static_destroy. Decode runs only where a test
        // module is on disk.
        let service = PluginService::new(p.name, Vfs::new());
        let svc = service.get_c_api() as *const RVService;
        if let Some(init) = plugin.static_init {
            init(svc);
        }
        let user_data = (plugin.create.unwrap())(svc);
        assert!(!user_data.is_null(), "{}: create returned null", p.name);

        if let Some(rel) = p.module_rel {
            match std::fs::canonicalize(resolve(p.module_env, rel)) {
                Ok(module) => {
                    let c_url = CFixedString::from_str(module.to_str().unwrap());
                    let rc = (plugin.open.unwrap())(user_data, c_url.as_ptr(), 0, svc);
                    assert_eq!(rc, 0, "{}: open failed for {}", p.name, module.display());
                    assert!(decode_ok(plugin, user_data), "{}: decoded no audio frames", p.name);
                }
                Err(_) => eprintln!("skipping {} decode: module not found", p.name),
            }
        }

        (plugin.destroy.unwrap())(user_data);
        if let Some(static_destroy) = plugin.static_destroy {
            static_destroy();
        }
    }

    assert!(checked > 0, "no cohort plugin .so was built; nothing verified");
}
