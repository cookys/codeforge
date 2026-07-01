//! Mnemos client — CodeForge as the first streaming source for the Gen-2
//! active-memory daemon (Mnemos).
//!
//! Three responsibilities, mirroring the closed loop:
//!   - `ship`     — L2 daily ledger → `POST /v1/ingest/ledger` (atom 萃取的輸入)
//!   - `context`  — `GET /v1/atoms/context` → markdown 注入 SessionStart
//!   - `cite`     — `POST /v1/atoms/:id/cite` → citation write-back
//!
//! Contract truth: `cookys/mnemos/docs/specs/10-source-contract.md` (§3 envelope,
//! §5.1 ledger, §11 cite) cross-checked against Mnemos `crates/mnemos/src/api.rs`.

pub mod autocite;
pub mod cite;
pub mod cite_detect;
pub mod config;
pub mod context;
pub mod digest;
pub mod evidence;
pub mod health;
pub mod ledger;
pub mod state;
pub mod transport;

/// Generate a new ULID string (26-char Crockford base32) for `ship_id` / `cite_id`.
pub fn new_ulid() -> String {
    ulid::Ulid::new().to_string()
}
