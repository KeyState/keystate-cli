//! Output: path rules and the durable local-file sink.
//!
//! One concern per module: path and filename rules ([`layout`]) and the
//! atomic, history-preserving writer ([`sink`]).

mod layout;
mod sink;

pub use layout::{Layout, sanitize_realm_component};
pub use sink::{CheckStatus, LocalFileSink, Written};
