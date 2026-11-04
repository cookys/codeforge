//! G36 usage collector (doc/plans/2026-11-03-usage-collector.md).
//!
//! Phase 1: account aliases and per-account quota snapshots written by the
//! statusline, read back for the usage row.

pub mod alias;
pub mod autocollect;
pub mod collect;
pub mod price;
pub mod quota;
pub mod quota_mirror;
pub mod store;
pub mod summary;
