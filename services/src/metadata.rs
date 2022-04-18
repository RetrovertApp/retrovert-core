use crate::ffi_gen::MetadataId;

pub struct Metadata {
    _dummy: u32,
}

impl Metadata {
    pub fn new() -> Metadata {
        Metadata { _dummy: 0 }
    }

    pub fn create_url(&mut self, _url: &str) -> MetadataId {
        0
    }

    pub fn set_tag(&mut self, _id: MetadataId, _tag: &str, _data: &str) {}

    pub fn set_tag_f64(&mut self, _id: MetadataId, _tag: &str, _data: f64) {}

    pub fn add_subsong(&mut self, _parent_id: MetadataId, _index: u32, _name: &str, _length: f32) {}

    pub fn add_sample(&mut self, _parent_id: MetadataId, _text: &str) {}

    pub fn add_instrument(&mut self, _parent_id: MetadataId, _text: &str) {}
}
