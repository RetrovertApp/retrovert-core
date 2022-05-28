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

struct PlaylistEntry {
    entries: Vec<Entries>,
}

//

/*
foobar.zip/dream.adf/[extracted.mod]


entries = {
    Entry {
        entry_type: EntryType::Driver("local-fs"),
        entry_url: "foobar.zip",
    },
    Entry {
        entry_type: EntryType::Driver("zip"),
        entry_url: "dream.adf",
    }
    Entry {
        entry_type: EntryType::Driver("pro-wizard"),
        entry_url: "extracted.mod",
    }
}
*/
