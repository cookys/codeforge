use crossterm::style::Color;

pub struct Village {
    pub id: &'static str,
    pub display_name: &'static str,
    pub language: &'static str,
    pub pet_name: &'static str,
    pub color: Color,
    pub tagline: &'static str,
    pub ascii_small: &'static str, // 6 行 × ≤10 字（statusline 用）
    pub ascii_full: &'static str,  // 完整版（pet card / TUI 用）
}

impl Village {
    /// UX Pro ANSI256 RGB for this village (for owo-colors statusline)
    pub fn rgb(&self) -> (u8, u8, u8) {
        match self.id {
            "rust" => (0xD7, 0x5F, 0x00),       // 166 #D75F00 — Rust orange
            "typescript" => (0x5F, 0xAF, 0xFF), // 75  #5FAFFF — TS brand blue
            "python" => (0xFF, 0xD7, 0x00),     // 184 #FFD700 — Python gold
            "go" => (0x5F, 0xD7, 0xD7),         // 80  #5FD7D7 — Go cyan-teal
            "javascript" => (0xFF, 0xAF, 0x00), // 214 #FFAF00 — JS amber-yellow
            _ => (0xAF, 0x87, 0xD7),            // 140 #AF87D7 — default slate-violet
        }
    }
}

pub static VILLAGES: &[Village] = &[
    Village {
        id: "python",
        display_name: "Scriptorium Vast",
        language: "Python",
        pet_name: "Spam",
        color: Color::Yellow,
        tagline: "凡事皆物件，空白定義秩序",
        ascii_small: concat!(
            "  ,_^_,\n",
            " ((o,o))\n",
            "  ):::(\n",
            "  \" | \"\n",
            "  | V |\n",
            "  V   V"
        ),
        ascii_full: concat!(
            "    ,___,\n",
            "   ( o,o )\n",
            "   |):::(|\n",
            "    V---V\n",
            "    \"   \"\n",
            "    V V V"
        ),
    },
    Village {
        id: "typescript",
        display_name: "Border Garrison",
        language: "TypeScript",
        pet_name: "Blueprint",
        color: Color::Blue,
        tagline: "型別是合約，Promise 是承諾",
        ascii_small: concat!(
            "/\\___/\\\n",
            "(=^.^=)\n",
            " )   (\n",
            "(     )\n",
            " \\___/\n",
            "  u u"
        ),
        ascii_full: concat!(
            "  /\\___/\\\n",
            " ( =^.^= )\n",
            " (  ~~~  )\n",
            "  )     (\n",
            "  \\_____/\n",
            "   u   u"
        ),
    },
    Village {
        id: "rust",
        display_name: "The Forge-Ruins",
        language: "Rust",
        pet_name: "Ferris",
        color: Color::Red,
        tagline: "所有權是責任，錯誤是公民",
        // canonical: rust-lang/ferris-says，壓縮至 ART_W=10
        ascii_small: concat!(
            " _~^~^~_\n",
            "\\/  oo  \\/\n",
            "  '_ - _'\n",
            "   /---\\\n",
            "   |   |\n",
            "   ~   ~"
        ),
        ascii_full: concat!(
            "   _~^~^~_\n",
            "\\) / o o \\ (/\n",
            "  '_  -  _'\n",
            "  / '---' \\\n",
            "   |     |\n",
            "   ~     ~"
        ),
    },
    Village {
        id: "go",
        display_name: "Dockside Workshop",
        language: "Go",
        pet_name: "Gopher",
        color: Color::Cyan,
        tagline: "簡單即道，並發是天性",
        // canonical: belbomemo gist Go gopher，壓縮版
        ascii_small: concat!(
            "  (` `)\n",
            " >(. .)<\n",
            "  |   |\n",
            "  |___|\n",
            "  || ||\n",
            "  ~~ ~~"
        ),
        ascii_full: concat!(
            "   ,_____,\n",
            "  (  o o  )\n",
            "  ( > . < )\n",
            "   )     (\n",
            "  (_______)\n",
            "   || | ||"
        ),
    },
    Village {
        id: "javascript",
        display_name: "Strata Bazaar",
        language: "JavaScript",
        pet_name: "Wat",
        color: Color::AnsiValue(214),
        tagline: "null ≠ undefined，callback 是歷史",
        ascii_small: concat!(
            "  ??!??\n",
            " (#o_o#)\n",
            "  )   (\n",
            "  |   |\n",
            "  |___|\n",
            "  ~~~~~"
        ),
        ascii_full: concat!(
            "  ??!?!??\n",
            " (#o   o#)\n",
            "  (  ~  )\n",
            "  )     (\n",
            "  |     |\n",
            "  |_____|"
        ),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    /// statusline 右欄寬度（`render_full` 的 ART_W）
    const ART_W: usize = 10;

    #[test]
    fn art_keeps_leading_indentation() {
        // Rust 字串續行（行尾 `\`）會吃掉下一行開頭空白；圖案必須保留縮排。
        let rust = VILLAGES.iter().find(|v| v.id == "rust").unwrap();
        let full: Vec<&str> = rust.ascii_full.lines().collect();
        assert_eq!(full[0], "   _~^~^~_");
        assert_eq!(full[1], "\\) / o o \\ (/");
        let small: Vec<&str> = rust.ascii_small.lines().collect();
        assert_eq!(small[0], " _~^~^~_");
        assert_eq!(small[3], "   /---\\");
    }

    #[test]
    fn every_village_art_is_indented_and_fits() {
        for v in VILLAGES {
            for art in [v.ascii_small, v.ascii_full] {
                let lines: Vec<&str> = art.lines().collect();
                assert!(!lines.is_empty(), "{} art is empty", v.id);
                assert!(
                    lines.iter().any(|l| l.starts_with(' ')),
                    "{} art lost all indentation",
                    v.id
                );
            }
            for l in v.ascii_small.lines() {
                assert!(
                    UnicodeWidthStr::width(l) <= ART_W,
                    "{} small art line {:?} wider than {}",
                    v.id,
                    l,
                    ART_W
                );
            }
        }
    }
}
