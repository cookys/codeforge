//! Mirror of CodePower's `plugin_api::clan_content` trait + types.
//!
//! **Parity rule**: field order, types, and method signatures here MUST
//! match CodePower one-for-one. A shared crate is trigger-based per
//! Open Q A3 — until then, edits are manual and reviewed against the
//! CodePower source of truth.
//!
//! Reference: `~/projects/codepower/backend/src/plugin_api/clan_content.rs`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub type UserId = i64;
pub type ClanId = i64;
pub type BlueprintId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanSummary {
    pub id: ClanId,
    pub slug: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub nation_id: Option<i64>,
    pub member_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetBlueprint {
    pub id: BlueprintId,
    pub clan_id: ClanId,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub tagline: Option<String>,
    #[serde(default)]
    pub lore: Option<String>,
    #[serde(default)]
    pub npc_voice: Option<String>,
    pub version: i32,
    pub sprite_forms: serde_json::Value,
    pub evolution_tree: serde_json::Value,
    pub attributes: serde_json::Value,
    #[serde(default)]
    pub palette: Option<serde_json::Value>,
    pub status: String,
    pub asset_type: String,
}

#[async_trait]
pub trait ClanContentProvider: Send + Sync {
    async fn available_clans(
        &self,
        user_id: UserId,
    ) -> anyhow::Result<Vec<ClanSummary>>;

    async fn clan_pets(
        &self,
        clan_id: ClanId,
    ) -> anyhow::Result<Vec<PetBlueprint>>;

    async fn blueprint(
        &self,
        id: BlueprintId,
    ) -> anyhow::Result<Option<PetBlueprint>>;
}
