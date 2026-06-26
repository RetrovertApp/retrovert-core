//! Hand-written impls api_gen does not emit for the plugin vtables:
//! `Copy`/`Clone` (a vtable is just pointers — cheap to copy), `Send`/`Sync`
//! (the loader keeps the vtable alive for the whole process), and the
//! name/version string accessors.

// ponytail: this exists only because api_gen emits no methods or derives.
// The real fix is teaching api_gen to emit `derive(Copy, Clone, Debug)`;
// do that when the def-file format grows derive support, then delete this.
use crate::{AudioFormat, OutputPlugin, PlaybackPlugin, ResamplePlugin};
use std::borrow::Cow;
use std::ffi::CStr;
use std::fmt;

macro_rules! plugin_vtable_impls {
    ($t:ty) => {
        impl Copy for $t {}
        impl Clone for $t {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl fmt::Debug for $t {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_struct(stringify!($t))
                    .field("name", &self.get_name())
                    .field("version", &self.get_version())
                    .finish()
            }
        }
        impl $t {
            pub fn get_name(&self) -> Cow<'_, str> {
                unsafe { CStr::from_ptr(self.name) }.to_string_lossy()
            }
            pub fn get_version(&self) -> Cow<'_, str> {
                unsafe { CStr::from_ptr(self.version) }.to_string_lossy()
            }
        }
    };
}

plugin_vtable_impls!(PlaybackPlugin);
plugin_vtable_impls!(OutputPlugin);
plugin_vtable_impls!(ResamplePlugin);

// The vtable is immutable for the plugin's lifetime, so sharing the struct
// across the decoder/output threads is sound.
unsafe impl Send for PlaybackPlugin {}
unsafe impl Sync for PlaybackPlugin {}
unsafe impl Send for ResamplePlugin {}
unsafe impl Sync for ResamplePlugin {}

// api_gen emits no derives; core compares audio formats by value.
impl PartialEq for AudioFormat {
    fn eq(&self, other: &Self) -> bool {
        self.audio_format == other.audio_format
            && self.channel_count == other.channel_count
            && self.sample_rate == other.sample_rate
    }
}
