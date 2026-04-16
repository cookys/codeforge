use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::power::CharacterStats;
use crate::pet::village::Village;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PetState {
    pub village: String,
    pub name: String,
    pub level: u32,
    pub xp: u32,
    pub xp_to_next: u32,
    pub atk: u32,
    pub hp: u32,
    pub def: u32,
    pub sup: u32,
    pub ver: u32,
}

impl PetState {
    pub fn new(village: &Village, stats: &CharacterStats) -> Self {
        Self {
            village: village.id.to_string(),
            name: village.pet_name.to_string(),
            level: 1,
            xp: 0,
            xp_to_next: 100,
            atk: stats.atk,
            hp: stats.hp,
            def: stats.def,
            sup: stats.sup,
            ver: stats.ver,
        }
    }

    pub fn exists(conn: &Connection) -> Result<bool> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pet WHERE id = 1",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        Ok(count > 0)
    }

    pub fn load(conn: &Connection) -> Result<Self> {
        conn.query_row(
            "SELECT village, name, level, xp, xp_to_next, atk, hp, def, sup, ver FROM pet WHERE id = 1",
            [],
            |row| {
                Ok(PetState {
                    village: row.get(0)?,
                    name: row.get(1)?,
                    level: row.get::<_, u32>(2)?,
                    xp: row.get::<_, u32>(3)?,
                    xp_to_next: row.get::<_, u32>(4)?,
                    atk: row.get::<_, u32>(5)?,
                    hp: row.get::<_, u32>(6)?,
                    def: row.get::<_, u32>(7)?,
                    sup: row.get::<_, u32>(8)?,
                    ver: row.get::<_, u32>(9)?,
                })
            },
        ).map_err(|e| anyhow::anyhow!("載入寵物失敗：{}", e))
    }

    pub fn save(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO pet (id, village, name, level, xp, xp_to_next, atk, hp, def, sup, ver, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))",
            rusqlite::params![
                self.village, self.name, self.level, self.xp, self.xp_to_next,
                self.atk, self.hp, self.def, self.sup, self.ver,
            ],
        )?;
        Ok(())
    }

    /// 增加 XP，自動升等
    pub fn add_xp(&mut self, amount: u32) {
        self.xp += amount;
        while self.xp >= self.xp_to_next {
            self.xp -= self.xp_to_next;
            self.level += 1;
            // 升等：xp_to_next 增加 50%，屬性各 +1
            // cap at 10M to prevent f32 precision loss and u32 overflow infinite loop
            self.xp_to_next = ((self.xp_to_next as f64 * 1.5) as u64).min(10_000_000) as u32;
            self.atk += 1;
            self.hp  += 1;
            self.def += 1;
            self.sup += 1;
            self.ver += 1;
        }
    }
}
