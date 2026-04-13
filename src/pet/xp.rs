use anyhow::Result;
use crate::db;
use crate::pet::state::PetState;

/// 給寵物加 XP（記憶活動觸發）
pub fn award(ctx: &db::Context, source: &str, amount: i64, detail: Option<&str>) -> Result<()> {
    let conn = ctx.open_db()?;

    // 記錄 XP 事件
    conn.execute(
        "INSERT INTO xp_events (source, xp_delta, detail) VALUES (?1, ?2, ?3)",
        rusqlite::params![source, amount, detail],
    )?;

    // 若有寵物，更新 XP
    if PetState::exists(&conn)? {
        let mut pet = PetState::load(&conn)?;
        pet.add_xp(amount.max(0) as u32);
        pet.save(&conn)?;
    }

    Ok(())
}
