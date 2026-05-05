/// PowerProvider trait — Phase 1 使用 StubPowerProvider
/// Phase 3 整合 CodePower scanner 時換成 CodePowerScannerProvider，不改其他程式碼
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CharacterStats {
    pub atk: u32, // Fluency（程式流暢度）
    pub hp: u32,  // Activity（提交活躍度）
    pub def: u32, // Integrity（程式可靠性）
    pub sup: u32, // Reach（跨域廣度影響力）
    pub ver: u32, // Breadth（語言/技術廣度）
}

impl Default for CharacterStats {
    fn default() -> Self {
        Self {
            atk: 10,
            hp: 10,
            def: 10,
            sup: 10,
            ver: 10,
        }
    }
}

impl CharacterStats {
    #[allow(dead_code)] // Used by Phase 3e crafting preview; keep as public API.
    pub fn total(&self) -> u32 {
        self.atk + self.hp + self.def + self.sup + self.ver
    }

    /// 以 variance 範圍隨機浮動（用於 adopt 時首抽）
    pub fn with_variance(mut self, variance: u32) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        // 簡單 xorshift32 偽隨機（不需要密碼學品質）
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let mut x = if seed == 0 { 12345 } else { seed };
        let apply = |base: u32, x: &mut u32| -> u32 {
            *x ^= *x << 13;
            *x ^= *x >> 17;
            *x ^= *x << 5;
            let delta = (*x % (variance * 2 + 1)) as i64 - variance as i64;
            (base as i64 + delta).max(1) as u32
        };
        self.atk = apply(self.atk, &mut x);
        self.hp = apply(self.hp, &mut x);
        self.def = apply(self.def, &mut x);
        self.sup = apply(self.sup, &mut x);
        self.ver = apply(self.ver, &mut x);
        self
    }
}

pub trait PowerProvider: Send + Sync {
    fn character_power(&self, repo: Option<&Path>) -> CharacterStats;
}

/// Phase 1 stub — 回傳固定基礎屬性 + 首抽 variance
pub struct StubPowerProvider {
    pub base_stats: CharacterStats,
    pub variance: u32,
}

impl Default for StubPowerProvider {
    fn default() -> Self {
        Self {
            base_stats: CharacterStats {
                atk: 15,
                hp: 15,
                def: 15,
                sup: 15,
                ver: 15,
            },
            variance: 5,
        }
    }
}

impl PowerProvider for StubPowerProvider {
    fn character_power(&self, _repo: Option<&Path>) -> CharacterStats {
        self.base_stats.clone().with_variance(self.variance)
    }
}
