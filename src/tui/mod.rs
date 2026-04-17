//! TUI renderer — Phase 2c.
//!
//! Top-level module for the alt-screen terminal UI. Pure data providers
//! + layout + terminal guard are in submodules; panel renderers arrive
//! in P4 and the event loop in P5.

pub mod guard;
pub mod layout;
pub mod local_map;
pub mod panels;
