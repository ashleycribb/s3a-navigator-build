/// `hit_test` result (§5.1). Pure: does not change handle state (DD-97).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Continue,
    Floor(u32),
    Select { paper_id: String },
    Link { href: String },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    HyperTile,
    Edge,
    Unsupported,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HyperTile => "hyper-tile",
            Self::Edge => "edge",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FirstView {
    Manifest,
    Header,
    Banner,
}

impl FirstView {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Header => "header",
            Self::Banner => "banner",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Floor {
    pub index: u32,
    pub type_id: u32,
    pub record_count: u32,
    pub size_bytes: u32,
    pub payload_hash_short: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ManifestState {
    Text(String),
    Absent,
    NotUtf8,
    Unsupported { banner: String },
}
