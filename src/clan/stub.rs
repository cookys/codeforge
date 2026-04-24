//! `StubClanContentProvider` — returns empty data for every call.
//!
//! Used as a fallback when CodePower is unreachable, the protocol
//! handshake fails, or the `PET_BLUEPRINTS_ENABLED` flag is off. Lets
//! the rest of CodeForge build against the trait without depending on
//! network reachability.

use async_trait::async_trait;

use super::provider::{
    BlueprintId, ClanContentProvider, ClanId, ClanSummary, PetBlueprint, UserId,
};

pub struct StubClanContentProvider;

#[async_trait]
impl ClanContentProvider for StubClanContentProvider {
    async fn available_clans(&self, _user_id: UserId) -> anyhow::Result<Vec<ClanSummary>> {
        Ok(Vec::new())
    }

    async fn clan_pets(&self, _clan_id: ClanId) -> anyhow::Result<Vec<PetBlueprint>> {
        Ok(Vec::new())
    }

    async fn blueprint(&self, _id: BlueprintId) -> anyhow::Result<Option<PetBlueprint>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_empty_lists() {
        let s = StubClanContentProvider;
        assert!(s.available_clans(1).await.unwrap().is_empty());
        assert!(s.clan_pets(1).await.unwrap().is_empty());
        assert!(s.blueprint(1).await.unwrap().is_none());
    }
}
