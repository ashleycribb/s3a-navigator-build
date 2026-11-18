/// Pack verification report (§5.1). Status and seal_status strings follow
/// the reader-core mapping in the FFI table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KiteVerificationReport {
    pub status: String,
    pub seal_status: String,
    pub tiles_crc_failed: Vec<u32>,
    pub invalid_tile_indices: Vec<u32>,
    pub break_index: Option<u32>,
    pub torn_tile_indices: Vec<u32>,
    pub quarantine_reason: Option<String>,
    pub detail: String,
}

impl KiteVerificationReport {
    pub(crate) fn unsigned_unsealed() -> Self {
        Self {
            status: "unsigned".into(),
            seal_status: "unsealed".into(),
            tiles_crc_failed: Vec::new(),
            invalid_tile_indices: Vec::new(),
            break_index: None,
            torn_tile_indices: Vec::new(),
            quarantine_reason: None,
            detail: String::new(),
        }
    }
}
