use termcolor::Color;

pub struct Village {
    pub id: &'static str,
    pub display_name: &'static str,
    pub language: &'static str,
    pub pet_name: &'static str,
    pub color: Color,
    pub tagline: &'static str,
    pub ascii_small: &'static str,  // ≤ 6 行 × 12 字（statusline 用）
    pub ascii_full: &'static str,   // 完整版（pet card 用）
}

pub static VILLAGES: &[Village] = &[
    Village {
        id: "python",
        display_name: "Scriptorium Vast",
        language: "Python",
        pet_name: "Spam",
        color: Color::Yellow,
        tagline: "凡事皆物件，空白定義秩序",
        ascii_small: "\
 ,_,\n\
(o,o)\n\
/)__)\n\
 \" \"",
        ascii_full: "\
    ,___,\n\
   (o , o)\n\
   /)___(\\  \n\
  // | | \\\\\n\
   \"\"   \"\"",
    },
    Village {
        id: "typescript",
        display_name: "Border Garrison",
        language: "TypeScript",
        pet_name: "Blueprint",
        color: Color::Blue,
        tagline: "型別是合約，Promise 是承諾",
        ascii_small: "\
/\\___/\\\n\
( o  o)\n\
(  ^^  )\n\
 \\____/",
        ascii_full: "\
  /\\____/\\\n\
 ( o    o )\n\
 (   ^^^^  )\n\
  \\______/\n\
   |  ||  |",
    },
    Village {
        id: "rust",
        display_name: "The Forge-Ruins",
        language: "Rust",
        pet_name: "Ferris",
        color: Color::Red,
        tagline: "所有權是責任，錯誤是公民",
        ascii_small: "\
  /  \\\n\
 (><><)\n\
d/|  |\\b\n\
  ~  ~",
        ascii_full: "\
    /  \\\n\
   (><><)\n\
  d/|  |\\b\n\
   d|  |b\n\
    ~  ~",
    },
    Village {
        id: "go",
        display_name: "Dockside Workshop",
        language: "Go",
        pet_name: "Gopher",
        color: Color::Cyan,
        tagline: "簡單即道，並發是天性",
        ascii_small: "\
 (o)(o)\n\
 {    }\n\
  \\  /\n\
  /  \\",
        ascii_full: "\
  (o)(o)\n\
  {    }\n\
   \\  /\n\
   /  \\\n\
  (====)",
    },
    Village {
        id: "javascript",
        display_name: "Strata Bazaar",
        language: "JavaScript",
        pet_name: "Wat",
        color: Color::Ansi256(214), // 橙色
        tagline: "null ≠ undefined，callback 是歷史",
        ascii_small: "\
 ??!?!\n\
(#o_o#)\n\
 |   |\n\
 |___|",
        ascii_full: "\
  ??!?!?\n\
 (#o_o#)\n\
  |   |\n\
  | W |\n\
  |___|",
    },
];
