# CodeForge Nation System — P2P Design Spec

> 建立：2026-04-16
> 狀態：Draft / Vision（Think Tank 審查完成 2026-04-16）
> 前置：codeforge-mud-engine.md（Phase 1-2 單機架構）

---

## Vision

CodeForge 是玩家的護照。Nation 是各自獨立的國家。
玩家帶著自己的 pet 到不同國家探險、接 quest、拿當地獨特寵物——
底層機制是「你的生產力讓你在那個國家立足」。

**整個行為的驅動力是遊戲，只是遊戲積分來源是真實的生產能力。**

---

## 三個角色

### Nation（國家）
- 任何公司/組織自架的 CodePower open source instance
- 定義自己的評分 plugin（FAIR Score 是預設，可完全客製）
- 發行自己的獨特寵物、badge、稱號
- repo 掃描完全在 Nation 內部基礎設施執行，原始 code 永不外流
- 對外只輸出：玩家憑證（簽名的 badge/level/pet）

### Codeforge（玩家護照）
- 玩家本地 CLI
- 可同時登錄多個 Nation
- 聚合來自不同 Nation 的憑證
- 本地 game state（pet、memory、stats）

### Player（玩家）
- 主動選擇要去哪個 Nation 登錄
- 授權該 Nation 掃描指定 repo（explicit opt-in）
- 動機：拿那個 Nation 的獨特寵物 / badge / 稱號
- 副產品：對 Nation 展示了真實的能力水準

---

## 隱私設計

```
Player repo  →  Nation 內部掃描（不離開 Nation 環境）
                      ↓
             Nation 簽名的憑證（只有分數/badge，無原始 code）
                      ↓
             Codeforge 本地收藏
```

- Nation 看不到其他 Nation 的憑證
- 我們（開源維護者）完全看不到任何人的 repo 或分數
- Player 控制授權哪個 Nation、授權哪些 repo

---

## 獨特寵物系統（核心遊戲鉤子）

每個 Nation 有自己的**限定寵物**，只有達到該 Nation 標準才能獲得：

```
CompA（Rust 專精公司）  →  限定：鐵鏽色鍛造師 Ferrus
CompB（ML 公司）        →  限定：神經網路精靈 Neurix
CompC（Security 公司）  →  限定：加密烏鴉 Cipher
開源社群 Nation         →  限定：章魚（多語言）Octo
```

寵物稀有度由 Nation 自定義：
- 基礎登錄 → 普通 egg（孵化後是該 Nation 基本款）
- 達到特定 quest → 進化形態
- 頂尖評分 → Legendary 版本（發光/特殊動畫）

**玩家動機：「我想收集那個公司的傳說寵物」**

---

## Nation 評分 Plugin

FAIR Score 從硬編碼公式變成可替換 plugin：

```
interface NationPlugin {
  name: string
  version: string
  scan(repo: RepoData) -> PlayerScore
  quests: Quest[]           // Nation 自定義任務
  pets: PetDefinition[]     // Nation 限定寵物規格
  badges: BadgeDefinition[] // Nation 限定徽章
}
```

任何 Nation 可以：
- 沿用標準 FAIR Score（預設）
- 調整維度權重（我們更重視 test coverage）
- 新增自定義維度
- 完全換掉用自己的邏輯

---

## Quest 系統（Nation 自定義）

Nation 可以設計自己的 quest 吸引玩家：

```
CompA Quest 範例：
  [ ] 在 Rust repo 達到 0 clippy warning  → Bronze Ferrus
  [ ] 連續 30 天有 commit activity         → Silver Ferrus
  [ ] PR review 率 > 80%                  → Gold Ferrus
  [ ] 通過 CompA 技術審核                  → Legendary Ferrus ✨
```

Quest 完成 = 玩家拿到憑證 + 可能被 CompA 注意到（副產品）

---

## 憑證格式（草案）

Nation 簽發的憑證是輕量 JSON，Nation 私鑰簽名：

```json
{
  "nation": "compA.example.com",
  "player_id": "hash_of_public_key",
  "issued_at": "2026-04-16",
  "level": 7,
  "badges": ["rust-warrior", "ci-master"],
  "pet": {
    "species": "ferrus",
    "rarity": "gold",
    "name": "Cinder"
  },
  "signature": "..."
}
```

驗證方式：任何人可以用 Nation 公鑰驗證憑證真偽（無需中央伺服器）。

---

## 開源架構

```
codeforge/          ← 玩家本地 client（本 repo）
codepower/          ← Nation 基礎設施（open source，公司自架）
  └── plugins/
      └── fair-score/   ← 預設 plugin
      └── (任何人可貢獻)
```

完全 P2P：
- 沒有中央伺服器
- Nations 之間不需要溝通
- Codeforge 本地聚合憑證
- 憑證驗證是 cryptographic（不需要聯網）

---

## Think Tank 洞見（2026-04-16）

六角色審查後的關鍵結論，補充進原始設計。

### 碰撞洞見 1：Pet Breeding 不需要 Nation 通訊

Product 角色提出跨 Nation pet breeding 做為留存機制。Architect 確認 Nations 之間完全不通訊。

**這兩個不衝突**：Breeding 可以是純本地操作——把兩個本地簽名憑證合成一個 derived pet，由 Codeforge 本地簽名，不需要任何 Nation 參與。這讓 breeding 從「需要伺服器協調的功能」變成「架構上幾乎免費的機制」。

```
Credential(Nation A) + Credential(Nation B) → local merge → DerivedPet(signed by player keypair)
```

### 碰撞洞見 2：Nation Registry = 信任機制也是遊戲機制

QA 角色指出需要 Nation Registry 防 Sybil attack（假 Nation 自簽 Legendary）；Customer 角色指出 Nation 必須競爭讓玩家想去。

**兩個加在一起**：Registry 不只是防偽工具，它本身就是遊戲排行榜。被玩家評分高的 Nation 在 Registry 上有聲望，成為「知名 Nation」對公司有品牌價值。信任機制自然變成遊戲激勵。

收錄門檻：PR review 到社群維護的 `nations.toml`（不是中央伺服器，只是人工審核）。有聲望的 Nation 在 `codeforge nations list` 裡排名靠前。

### 碰撞洞見 3：Pet「口音」外化 Nation 聲望

UX 角色提出：來自不同 Nation 的 pet 在 statusline 說話方式不同，個性反映來源 Nation 的技術文化（Rust 公司 = 嚴肅直率；ML 公司 = 神秘；開源基金會 = 熱情）。

Customer 角色指出：玩家只有在 Nation 有聲望、quest 夠難時才以蒐集為傲。

**兩個加在一起**：pet 的口音是 Nation 聲望的外化——你的 pet 說什麼話，就在向別人展示你去過哪些地方。這比靜態 badge 更有生命感，且成本低（Phase 2 就能加，不需要新架構）。

---

## 設計決策（Think Tank 後）

### 1. Player Identity → ed25519 keypair

玩家本地生成 ed25519 keypair，存在 Codeforge 本地資料庫。Public key 是跨 Nation 的身份。

- **優點**：純 P2P，無中央依賴
- **必須配套**：加密備份（seed phrase 或 user-controlled 儲存）。Key loss = 所有 pet 消失，是最大留存殺手
- **不用**：GitHub OAuth（引入中央依賴，破壞 P2P 前提）

### 2. Nation Discovery → 社群 nations.toml

```bash
codeforge nations list        # 顯示已知 Nation 列表（本地快取）
codeforge nations update      # 從社群 nations.toml 更新列表
codeforge nations info <url>  # 查看特定 Nation 詳情
```

`nations.toml` 是 GitHub 上社群維護的檔案，收錄需要 PR review。純 P2P discovery 不存在——坦誠面對這個限制，提供務實解法。

### 3. Pet 不「旅行」

Pet 不需要「到」另一個 Nation。Pet 是本地的簽名憑證 blob，存在 Codeforge SQLite。其他玩家用簽發 Nation 的公鑰驗證真偽。No Nation-to-Nation communication。

### 4. 第一個 Nation = CodePower 本身

不需要等別人。CodePower 已有 scanner + FAIR Score + 評分邏輯，把它變成第一個示範 Nation，第一批限定 pet 從這裡產生。

### 5. 優先序

```
Phase 2（早期）：
  - Player keypair + 備份機制
  - nations.toml + codeforge nations list
  - Nation Registry（nations.toml PR review 即可）
  - Pet 口音系統（Nation 個性 → pet 說話風格）

Phase 3：
  - Pet breeding（本地 derived pet）
  - Nation 聲望排行（Registry + 玩家評分）
  - Seasonal quest rotation

Phase 4+：
  - 公開 player profile（可選）
  - 跨 Nation 活動
```

---

## 已知風險與緩解

| 風險 | 嚴重度 | 緩解 |
|------|--------|------|
| Sybil attack（假 Nation 自簽 Legendary） | 🔴 高 | nations.toml PR review 作為信任錨點 |
| Repo farming（假 repo 刷 quest） | 🔴 高 | 時間加權 commit velocity，非快照閾值 |
| Key loss（玩家失去所有 pet） | 🔴 高 | 加密備份為必要功能，非 optional |
| Nation 消失（憑證無法驗證） | 🟡 中 | Registry 保留已下線 Nation 的公鑰歷史 |
| Quest gaming（#[allow(...)] 規避 clippy） | 🟡 中 | 複合 quest 條件，社群可 flag 劣質 quest |
| Silent scanner death（玩家不知道掃描失敗） | 🟡 中 | Nation 需要最低監控：health endpoint + job status |
| 冷啟動（第一批玩家動機不足） | 🟡 中 | 第一個 Nation = CodePower，靠關係招募前 10 人 |

---

## 未解問題（留待後續）

1. **Credential revocation**：Nation 撤銷已發出的 pet 時，有沒有機制？還是「發出即永久」？
2. **Cross-Nation 活動**：Nations 之間完全獨立是否是最終形態，或未來有協議讓它們互認？
3. **公開 profile**：玩家的 Nation 收藏要怎麼對外展示？（portfolio 用途）
4. **Nation 更新評分規格**：舊憑證用舊規格計算，新加入玩家用新規格，公平性怎麼處理？
