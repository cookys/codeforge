//! Clan integration — CodeForge consumer side of the CodePower
//! `ClanContentProvider` contract.
//!
//! Phase 2 skeleton (Plan A
//! `~/projects/codepower/doc/projects/2026-04-24-clan-mvp-and-provider-trait/`):
//! types + trait + stub impl + HTTP client scaffold. Not yet wired into
//! `src/pet/village.rs` — that integration lands in Plan B Phase 6.
//!
//! See `doc/plans/2026-04-24-clan-content-provider-integration.md` and
//! `~/projects/codepower/doc/specs/clan-content-protocol-v1.md` for the
//! canonical protocol definition.

pub mod http;
pub mod provider;
pub mod stub;

// Public-API skeleton — re-exports in place ahead of village.rs wire-up
// (Plan B Phase 6). Suppresses unused-imports until consumers land.
#[allow(unused_imports)]
pub use http::HttpClanContentProvider;
#[allow(unused_imports)]
pub use provider::{BlueprintId, ClanContentProvider, ClanId, ClanSummary, PetBlueprint, UserId};
#[allow(unused_imports)]
pub use stub::StubClanContentProvider;
