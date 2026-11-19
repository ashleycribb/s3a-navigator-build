//! s3a-pack-kite: KiteContext, KiteError, and hyper-tile / Edge open/render
//! (build-plan step 8, first slice).
//!
//! Every archive open path uses `s3a_engine::MmapReader::from_bytes` (DD-75).
//! The pack does not copy `tour.rs`'s type-82 consent branch or its
//! stride-blind 128 KiB unsealed walker.

mod consent;
mod context;
mod error;
mod open;
mod page;
mod render;
mod report;
mod types;
mod verify;

pub use consent::{ConsentClass, ConsentEval};
pub use context::{Clock, KiteContext, ProcessClock, REQUIRED_UA_FONTS};
pub use error::KiteError;
pub use open::{
    close, hit_test, open, open_verified, proceed_to_warehouse, verification_report, Handle,
};
pub use page::render_page;
pub use render::{
    render_floor_detail, render_floor_transition, render_manifest, render_warehouse,
    RenderFrame, SemanticNode, TextRun, PAINT_BACKGROUND,
};
pub use report::KiteVerificationReport;
pub use types::Hit;
pub use verify::DerivedVerification;
