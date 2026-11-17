//! s3a-pack-kite skeleton (build-plan step 3).
//!
//! `KiteContext` holds the required UA font set. `KiteError` is a message
//! plus a stable code. Open/render APIs land in later steps.

mod context;
mod error;

pub use context::{Clock, KiteContext, ProcessClock, REQUIRED_UA_FONTS};
pub use error::KiteError;
