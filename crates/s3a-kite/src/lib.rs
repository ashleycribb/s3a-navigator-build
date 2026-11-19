//! `s3a-kite`: the S3A Navigator host binary and Host HTTP API (SPEC §5.2).

pub mod candidates;
pub mod consent;
pub mod egress;
pub mod fetch;
pub mod flags;
pub mod fonts;
pub mod hooks;
pub mod http;
pub mod index;
pub mod latch;
pub mod lrs;
pub mod paths;
pub mod png;
pub mod retry;
pub mod server;
pub mod snapshot;
pub mod timefmt;
pub mod token;
pub mod window;
pub mod write_path;
