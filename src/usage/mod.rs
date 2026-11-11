//! G36 usage collector (doc/plans/2026-11-03-usage-collector.md).
//!
//! Phase 1: account aliases and per-account quota snapshots written by the
//! statusline, read back for the usage row.

pub mod agy;
pub mod alias;
pub mod autocollect;
pub mod collect;
pub mod fleet_quota;
pub mod names;
pub mod price;
pub mod quota;
pub mod quota_mirror;
pub mod quota_mirror_read;
pub mod quota_view;
pub mod reset_rule;
pub mod store;
pub mod summary;
pub mod usage_v1;

#[cfg(test)]
mod fleet_quota_tests;
#[cfg(test)]
mod quota_view_tests;
#[cfg(test)]
mod usage_v1_tests;
