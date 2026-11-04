//! G40 display of the quota mirror: facts per alias, `usage accounts` lines, statusline chips.
//! A port of `plugins/forge-stats/hooks/quota-view.ts` (the reviewed reference); pure, no I/O, no colour codes
//! (callers map `Style` to a colour). Spec: `doc/plans/2026-11-03-g40-usage-accounts.md` section 3-6 and 10.

use super::quota_mirror::{is_tight, tightest, token_warn, MergedQuota, MergedWindow};
use super::quota_mirror_read::{QEntry, QuotaState};
use serde_json::{json, Value};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const DAY_S: i64 = 86400;
/// Show a window's age only past this (PM ruling 3).
pub const AGE_SHOW_S: f64 = 30.0 * 60.0;
/// Below this width a block is one line (section 6.2).
pub const COMPACT_COLS: usize = 60;
/// Below this width the age is dropped first (section 6.2).
pub const AGE_MIN_COLS: usize = 80;
const GAUGE_KINDS: [&str; 5] = ["rolling_5h", "weekly", "daily", "monthly", "credits"];
const LABEL_W: usize = 6;
const METER_W: usize = 10;
pub const CHIP_GAP: &str = "  ";
pub const CHIP_SPACE: &str = " ";

/// Looks up the quota words (`quota.*` in locales/*.yaml) for one locale.
pub struct Tr(pub String);
impl Tr {
    pub fn current() -> Self {
        Tr(rust_i18n::locale().to_string())
    }
    pub fn t(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut s = rust_i18n::t!(key, locale = self.0.as_str()).to_string();
        for (k, v) in args {
            s = s.replace(&format!("%{{{k}}}"), v);
        }
        s
    }
    fn q(&self, key: &str) -> String {
        self.t(&format!("quota.{key}"), &[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Dim,
    /// no data / cooling / unavailable
    Idle,
    Bold,
    /// the one yellow: need-you
    Warn,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub text: String,
    pub style: Style,
}
pub type Line = Vec<Part>;
fn p(text: impl Into<String>, style: Style) -> Part {
    Part {
        text: text.into(),
        style,
    }
}
fn pl(text: impl Into<String>) -> Part {
    p(text, Style::Plain)
}
pub fn width(l: &[Part]) -> usize {
    l.iter()
        .map(|s| UnicodeWidthStr::width(s.text.as_str()))
        .sum()
}
pub fn plain(l: &[Part]) -> String {
    l.iter().map(|s| s.text.as_str()).collect()
}
/// Cut a line to `cols` display columns (Unicode width, whole characters only, never a byte slice), ending
/// with `…` when anything was cut. Every rendered line goes through this last, so no form, footer or
/// message can be wider than the terminal.
pub fn clip(l: &[Part], cols: usize) -> Line {
    if width(l) <= cols {
        return l.to_vec();
    }
    if cols == 0 {
        return vec![];
    }
    let room = cols - 1; // keep one column for the ellipsis
    let mut out: Line = Vec::new();
    let mut used = 0usize;
    'parts: for s in l {
        let mut text = String::new();
        for c in s.text.chars() {
            let w = UnicodeWidthChar::width(c).unwrap_or(0);
            if used + w > room {
                if !text.is_empty() {
                    out.push(p(text, s.style));
                }
                break 'parts;
            }
            used += w;
            text.push(c);
        }
        out.push(p(text, s.style));
    }
    out.push(p("…", Style::Dim));
    out
}

fn fit(forms: Vec<Line>, cols: usize) -> Line {
    let pick = forms.iter().position(|f| width(f) <= cols);
    let mut forms = forms;
    let i = pick.unwrap_or(forms.len().saturating_sub(1));
    forms.swap_remove(i.min(forms.len().saturating_sub(1)))
}

// ---- shared text ----
fn pct_of(u: f64) -> String {
    format!("{}%", (u * 100.0).round() as i64)
}
fn with_commas(s: &str) -> String {
    let mut rev = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            rev.push(',');
        }
        rev.push(c);
    }
    rev.chars().rev().collect()
}
pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 {
        return with_commas(&format!("{}", n as i64));
    }
    let s = format!("{n:.2}");
    let (i, f) = s.split_once('.').unwrap_or((&s, "00"));
    format!("{}.{f}", with_commas(i))
}
fn left_of(w: &MergedWindow) -> Option<f64> {
    match (w.remaining, w.used, w.limit) {
        (Some(r), _, _) => Some(r),
        (None, Some(u), Some(l)) => Some((l - u).max(0.0)),
        _ => None,
    }
}
fn label_of(w: &MergedWindow, tr: &Tr) -> String {
    tr.q(match w.kind {
        "rolling_5h" => "w_5h",
        "weekly" => "w_weekly",
        "daily" => "w_daily",
        "monthly" => "w_monthly",
        "credits" => "w_credits",
        "rpm" => "w_rpm",
        _ => "w_tpm",
    })
}
fn is_gauge(w: &MergedWindow) -> bool {
    GAUGE_KINDS.contains(&w.kind)
}
fn win_tight(w: &MergedWindow) -> bool {
    is_gauge(w) && w.utilization.is_some_and(is_tight)
}
/// Minutes-granular countdown to `at_s` (epoch seconds, local clock), as `countdown` in model.ts.
pub fn countdown(at_s: f64, now_s: f64) -> String {
    let m = ((at_s - now_s) / 60.0).floor() as i64;
    if m < 0 {
        "—".to_string()
    } else if m < 60 {
        format!("{m}m")
    } else if m < 24 * 60 {
        format!("{}h{}m", m / 60, m % 60)
    } else {
        format!("{}d{}h", m / 1440, (m % 1440) / 60)
    }
}
fn age_text(age_s: f64, now_s: f64, tr: &Tr) -> String {
    tr.t("quota.age", &[("t", &countdown(now_s + age_s, now_s))])
}
fn pct_text(w: &MergedWindow, tr: &Tr) -> String {
    let est = if w.estimated {
        format!("{} ", tr.q("est"))
    } else {
        String::new()
    };
    format!("{est}{}", pct_of(w.utilization.unwrap_or(0.0)))
}
fn meter(ratio: f64, w: usize) -> (String, String) {
    let r = if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let on = ((r * w as f64).floor() as usize).min(w);
    ("▰".repeat(on), "▱".repeat(w - on))
}
fn pad_right(s: &str, w: usize) -> String {
    let n = UnicodeWidthStr::width(s);
    format!("{s}{}", " ".repeat(w.saturating_sub(n)))
}
fn left_text(tr: &Tr, n: f64, unit: Option<&str>) -> String {
    tr.t(
        "quota.left",
        &[("n", &fmt_num(n)), ("unit", unit.unwrap_or(""))],
    )
    .trim()
    .to_string()
}

// ---- facts ----
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub flag: bool,
    pub invalid: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Live,
    Refusal,
    Never,
    NoUsage,
    Dropped,
    TokenOnly,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Live => "live",
            Kind::Refusal => "refusal",
            Kind::Never => "never",
            Kind::NoUsage => "no_usage",
            Kind::Dropped => "dropped",
            Kind::TokenOnly => "token_only",
        }
    }
}
pub struct Fact<'a> {
    pub e: &'a QEntry,
    pub kind: Kind,
    pub tok: Option<Token>,
    pub main: Option<&'a MergedWindow>,
    pub tight: bool,
    pub group: u8,
}

fn token_of(m: &MergedQuota, now_s: i64, tr: &Tr) -> Option<Token> {
    if m.token_state == "invalid" {
        return Some(Token {
            text: tr.q("token_invalid"),
            flag: true,
            invalid: true,
        });
    }
    let exp = m.token_expires_at?;
    let tok = |text: String, flag: bool| Token {
        text,
        flag,
        invalid: false,
    };
    let w = token_warn(Some(exp), now_s);
    match w {
        Some(w) if w.level == "expired" => Some(tok(tr.q("token_expired"), true)),
        Some(w) if w.level == "urgent" => Some(tok(
            tr.t("quota.token_expires", &[("n", &w.days.to_string())]),
            true,
        )),
        _ => {
            let days = w.map_or((exp - now_s + DAY_S - 1).div_euclid(DAY_S), |w| w.days);
            Some(tok(
                tr.t("quota.token_expires", &[("n", &days.to_string())]),
                false,
            ))
        }
    }
}

pub fn facts_of<'a>(entries: &'a [QEntry], now_s: i64, tr: &Tr) -> Vec<Fact<'a>> {
    let mut facts: Vec<Fact> = entries
        .iter()
        .map(|e| {
            let m = &e.m;
            let tok = token_of(m, now_s, tr);
            let kind = if tok.as_ref().is_some_and(|t| t.invalid) {
                Kind::TokenOnly
            } else if !m.windows.is_empty() {
                Kind::Live
            } else if m.refusal.is_some() {
                Kind::Refusal
            } else if m.placeholder {
                Kind::Never
            } else if !m.had_window {
                Kind::NoUsage
            } else {
                Kind::Dropped
            };
            let tw = if kind == Kind::Live {
                tightest(m)
            } else {
                None
            };
            let main = tw.or_else(|| {
                (kind == Kind::Live)
                    .then(|| {
                        m.windows
                            .iter()
                            .find(|w| w.kind != "rpm" && w.kind != "tpm")
                            .or(m.windows.first())
                    })
                    .flatten()
            });
            let tight = tw.is_some_and(|w| w.utilization.is_some_and(is_tight));
            // section 6.2 block order: token flag, tight, 429, the rest; never/dropped last
            let group = if tok.as_ref().is_some_and(|t| t.flag) {
                0
            } else if tight {
                1
            } else if m.refusal.is_some() {
                2
            } else if matches!(kind, Kind::Never | Kind::NoUsage | Kind::Dropped) {
                4
            } else {
                3
            };
            Fact {
                e,
                kind,
                tok,
                main,
                tight,
                group,
            }
        })
        .collect();
    facts.sort_by(|a, b| a.group.cmp(&b.group).then_with(|| a.e.key.cmp(&b.e.key)));
    facts
}

// ---- pane lines ----
#[derive(Clone, Copy)]
struct WinForm {
    age: bool,
    full_reset: bool,
    meter: bool,
}
const WIN_FORMS: [WinForm; 4] = [
    WinForm {
        age: true,
        full_reset: true,
        meter: true,
    },
    WinForm {
        age: false,
        full_reset: true,
        meter: true,
    },
    WinForm {
        age: false,
        full_reset: false,
        meter: true,
    },
    WinForm {
        age: false,
        full_reset: false,
        meter: false,
    },
];

fn win_line(w: &MergedWindow, now_s: f64, tr: &Tr, f: WinForm) -> Line {
    let mut out: Line = vec![
        pl("  "),
        p(pad_right(&label_of(w, tr), LABEL_W), Style::Dim),
    ];
    match w.utilization {
        None => out.push(pl(left_text(tr, w.remaining.unwrap_or(0.0), w.unit))),
        Some(u) => {
            if f.meter && is_gauge(w) {
                let (on, off) = meter(u, METER_W);
                out.push(pl(on));
                out.push(p(off, Style::Idle));
                out.push(pl("  "));
            }
            out.push(pl(pct_text(w, tr)));
            if win_tight(w) {
                out.push(pl(format!(" {}", tr.q("tight"))));
            }
            if w.kind == "credits" {
                if let Some(l) = left_of(w) {
                    out.push(pl(format!("  {}", left_text(tr, l, w.unit))));
                }
            }
        }
    }
    if let Some(r) = w.resets_at_local {
        let tail = if f.full_reset {
            format!(" {}", tr.q("reset"))
        } else {
            String::new()
        };
        out.push(p(format!("  ◷ {}{tail}", countdown(r, now_s)), Style::Dim));
    } else if w.utilization.is_some() && is_gauge(w) {
        out.push(p(format!("  {}", tr.q("reset_unknown")), Style::Dim));
    }
    if f.age && w.age_s > AGE_SHOW_S {
        out.push(p(
            format!(" · {}", age_text(w.age_s, now_s, tr)),
            Style::Dim,
        ));
    }
    out
}

fn refusal_line(m: &MergedQuota, now_s: f64, tr: &Tr, full: bool) -> Line {
    let Some(r) = &m.refusal else {
        return vec![];
    };
    let tail = if full {
        format!(" {}", tr.q("retry"))
    } else {
        String::new()
    };
    vec![
        pl("  "),
        p(tr.q("cooldown"), Style::Idle),
        p(
            format!("  ◷ {}{tail}", countdown(r.end_s, now_s)),
            Style::Dim,
        ),
    ]
}

fn header_line(f: &Fact, cols: usize, tr: &Tr) -> Line {
    let m = &f.e.m;
    let name = || p(f.e.key.clone(), Style::Bold);
    let dot = || p(" · ", Style::Dim);
    let host: Option<Part> = m.multi_host.then(|| {
        let mut roles: Vec<&str> = Vec::new();
        for h in &m.hosts {
            if !roles.contains(&h.as_str()) {
                roles.push(h);
            }
        }
        p(
            tr.t("quota.host", &[("roles", &roles.join(&tr.q("role_sep")))]),
            Style::Dim,
        )
    });
    let tok: Vec<Part> = match &f.tok {
        None => vec![],
        Some(t) if t.flag => vec![p("▲", Style::Warn), pl(format!(" {}", t.text))],
        Some(t) => vec![p(t.text.clone(), Style::Dim)],
    };
    let join = |parts: Vec<Vec<Part>>| -> Line {
        let mut out: Line = vec![name(), pl("  "), p(m.level, Style::Dim)];
        for x in parts {
            out.push(dot());
            out.extend(x);
        }
        out
    };
    let mut forms: Vec<Line> = Vec::new();
    let with = |h: bool, t: bool| {
        let mut v: Vec<Vec<Part>> = Vec::new();
        if h {
            if let Some(hp) = &host {
                v.push(vec![hp.clone()]);
            }
        }
        if t && !tok.is_empty() {
            v.push(tok.clone());
        }
        v
    };
    forms.push(join(with(true, true)));
    if host.is_some() {
        forms.push(join(with(false, true)));
    }
    match &f.tok {
        Some(t) if !t.flag => forms.push(join(vec![])),
        Some(_) => forms.push(join(with(false, true))),
        None => {}
    }
    forms.push(join(vec![]));
    fit(forms, cols)
}

fn compact_line(f: &Fact, now_s: f64, cols: usize, tr: &Tr) -> Line {
    let m = &f.e.m;
    let name = || p(f.e.key.clone(), Style::Bold);
    let flag = || -> Line {
        match &f.tok {
            Some(t) if t.flag => vec![pl(" "), p("▲", Style::Warn), pl(format!(" {}", t.text))],
            _ => vec![],
        }
    };
    let state = |cd: bool, age: bool| -> Line {
        if let Some(r) = &m.refusal {
            let mut v = vec![pl(" "), p(tr.q("cooldown"), Style::Idle)];
            if cd {
                v.push(p(format!(" ◷ {}", countdown(r.end_s, now_s)), Style::Dim));
            }
            return v;
        }
        if matches!(f.kind, Kind::Never | Kind::NoUsage | Kind::Dropped) {
            return vec![pl(" "), p(tr.q("no_data"), Style::Idle)];
        }
        let Some(w) = f.main else { return vec![] };
        if f.kind == Kind::TokenOnly {
            return vec![];
        }
        let Some(_) = w.utilization else {
            return vec![pl(format!(
                " {} {}",
                label_of(w, tr),
                left_text(tr, w.remaining.unwrap_or(0.0), w.unit)
            ))];
        };
        let tight = if win_tight(w) {
            format!(" {}", tr.q("tight"))
        } else {
            String::new()
        };
        let mut v = vec![pl(format!(
            " {} {}{tight}",
            label_of(w, tr),
            pct_text(w, tr)
        ))];
        if cd {
            if let Some(r) = w.resets_at_local {
                v.push(p(format!(" ◷ {}", countdown(r, now_s)), Style::Dim));
            }
        }
        if age && w.age_s > AGE_SHOW_S {
            v.push(p(
                format!(" · {}", age_text(w.age_s, now_s, tr)),
                Style::Dim,
            ));
        }
        v
    };
    let build = |cd: bool, age: bool, fl: bool| -> Line {
        let mut l = vec![name()];
        l.extend(state(cd, age));
        if fl {
            l.extend(flag());
        }
        l
    };
    // the age never comes back in the compact forms: they are used only below AGE_MIN_COLS (design §6.2)
    let age = cols >= AGE_MIN_COLS;
    fit(
        vec![
            build(true, age, true),
            build(true, false, true),
            build(false, false, true),
            build(false, false, false),
            vec![name()],
        ],
        cols,
    )
}

/// The `usage accounts` body: one block per (provider, alias); the footer carries the unreadable count and the legend.
pub fn pane_lines(
    state: &QuotaState,
    entries: &[QEntry],
    cols: usize,
    now_s: i64,
    tr: &Tr,
) -> Vec<Line> {
    let now_f = now_s as f64;
    let facts = facts_of(entries, now_s, tr);
    let mut out: Vec<Line> = Vec::new();
    let (mut used_est, mut used_tight) = (false, false);
    let compact = cols < COMPACT_COLS;
    if facts.is_empty() {
        out.push(vec![p(format!("  {}", tr.q("empty")), Style::Dim)]);
    }
    for (i, f) in facts.iter().enumerate() {
        let m = &f.e.m;
        if compact {
            out.push(compact_line(f, now_f, cols, tr));
        } else {
            if i > 0 {
                out.push(vec![pl(" ")]);
            }
            out.push(header_line(f, cols, tr));
            match f.kind {
                Kind::Never => out.push(vec![p(format!("  {}", tr.q("never")), Style::Idle)]),
                Kind::NoUsage => out.push(vec![p(format!("  {}", tr.q("no_usage")), Style::Idle)]),
                Kind::Dropped => out.push(vec![p(format!("  {}", tr.q("dropped")), Style::Idle)]),
                Kind::TokenOnly => {}
                _ => {
                    if m.refusal.is_some() {
                        out.push(refusal_line(m, now_f, tr, true));
                    }
                    for w in &m.windows {
                        // design 6.2: below 80 columns the age goes first
                        let skip = usize::from(cols < AGE_MIN_COLS);
                        let forms = WIN_FORMS
                            .iter()
                            .skip(skip)
                            .map(|wf| win_line(w, now_f, tr, *wf))
                            .collect();
                        out.push(fit(forms, cols));
                    }
                }
            }
        }
        if f.kind == Kind::Live {
            for w in &m.windows {
                used_est |= w.estimated;
                used_tight |= win_tight(w);
            }
        }
    }
    if state.bad > 0 {
        out.push(vec![p(
            format!("  {}", tr.t("quota.bad", &[("n", &state.bad.to_string())])),
            Style::Dim,
        )]);
    }
    if state.skipped > 0 {
        out.push(vec![p(
            format!(
                "  {}",
                tr.t("quota.skipped", &[("n", &state.skipped.to_string())])
            ),
            Style::Dim,
        )]);
    }
    let mut legend: Vec<String> = Vec::new();
    if used_est {
        legend.push(tr.q("legend_est"));
    }
    if used_tight {
        legend.push(tr.q("legend_tight"));
    }
    if !legend.is_empty() {
        out.push(vec![pl(" ")]);
        out.push(vec![p(legend.join(&tr.q("sep")), Style::Dim)]);
    }
    out
}

/// `--json`: the interpreted list (after the age/reset drop rules), in display order; field names as in the contract.
pub fn accounts_json(state: &QuotaState, entries: &[QEntry], now_s: i64, tr: &Tr) -> Value {
    let facts = facts_of(entries, now_s, tr);
    let list: Vec<Value> = facts
        .iter()
        .map(|f| {
            let mut v = serde_json::to_value(&f.e.m).unwrap_or(Value::Null);
            if let Some(o) = v.as_object_mut() {
                o.insert("provider".into(), json!(f.e.provider));
                o.insert("alias".into(), json!(f.e.alias));
                o.insert("state".into(), json!(f.kind.name()));
                o.insert("tight".into(), json!(f.tight));
                o.insert(
                    "token_warn".into(),
                    serde_json::to_value(token_warn(f.e.m.token_expires_at, now_s))
                        .unwrap_or(Value::Null),
                );
            }
            v
        })
        .collect();
    json!({
        "schema": "codeforge.usage-accounts/1",
        "entries": list,
        "unreadable": state.bad,
        "skipped": state.skipped,
    })
}

// ---- band chips (section 6.1) ----
#[derive(Debug, Clone)]
pub struct BandChip {
    pub token: bool,
    pub key: String,
    pub alias: String,
    pub head: String,
    pub short: String,
    pub inactive: bool,
    pub reset: Option<String>,
    pub age: Option<String>,
}
pub struct Band {
    pub chips: Vec<BandChip>,
    pub no_data: usize,
    pub no_data_word: String,
}

/// Chips in section 6.1 order: token flag, tight, 429, the rest by utilization; plus the count of aliases with nothing to draw.
pub fn quota_band(entries: &[QEntry], now_s: i64, tr: &Tr) -> Band {
    let now_f = now_s as f64;
    let facts = facts_of(entries, now_s, tr);
    let mut ranked: Vec<(u8, f64, BandChip)> = Vec::new();
    let mut no_data = 0;
    for f in &facts {
        let m = &f.e.m;
        if let Some(t) = f.tok.as_ref().filter(|t| t.flag) {
            ranked.push((
                0,
                0.0,
                BandChip {
                    token: true,
                    key: f.e.key.clone(),
                    alias: f.e.alias.clone(),
                    head: t.text.clone(),
                    short: t.text.clone(),
                    inactive: false,
                    reset: None,
                    age: None,
                },
            ));
        }
        if f.kind == Kind::TokenOnly {
            continue;
        }
        if let Some(r) = &m.refusal {
            ranked.push((
                if f.tight { 1 } else { 2 },
                2.0,
                BandChip {
                    token: false,
                    key: f.e.key.clone(),
                    alias: f.e.alias.clone(),
                    head: tr.q("cooldown"),
                    short: tr.q("cooldown"),
                    inactive: true,
                    reset: Some(countdown(r.end_s, now_f)),
                    age: None,
                },
            ));
            continue;
        }
        if matches!(f.kind, Kind::Never | Kind::NoUsage | Kind::Dropped) {
            no_data += 1;
            continue;
        }
        let Some(w) = tightest(m) else { continue };
        let Some(u) = w.utilization else { continue }; // remaining-only credits, rpm/tpm: pane only
        let left = if w.kind == "credits" {
            left_of(w)
        } else {
            None
        };
        let est = if w.estimated {
            format!("{} ", tr.q("est"))
        } else {
            String::new()
        };
        let value = if w.kind != "credits" {
            pct_text(w, tr)
        } else if let Some(l) = left {
            format!("{est}{}", tr.t("quota.left_band", &[("n", &fmt_num(l))]))
        } else {
            tr.t("quota.used_pct", &[("p", &pct_text(w, tr))])
        };
        let tight_word = if f.tight {
            format!(" {}", tr.q("tight"))
        } else {
            String::new()
        };
        ranked.push((
            if f.tight { 1 } else { 3 },
            u,
            BandChip {
                token: false,
                key: f.e.key.clone(),
                alias: f.e.alias.clone(),
                head: format!("{} {value}{tight_word}", label_of(w, tr)),
                short: value,
                inactive: false,
                reset: w.resets_at_local.map(|r| countdown(r, now_f)),
                age: (w.age_s > AGE_SHOW_S).then(|| age_text(w.age_s, now_f, tr)),
            },
        ));
    }
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.2.key.cmp(&b.2.key))
    });
    Band {
        chips: ranked.into_iter().map(|r| r.2).collect(),
        no_data,
        no_data_word: tr.q("no_data"),
    }
}

#[derive(Clone, Copy)]
struct ChipForm {
    age: bool,
    countdown: bool,
}
fn chip_parts(glyph: &str, style: Style, value: &str) -> Line {
    vec![p(glyph, style), pl(CHIP_SPACE), pl(value)]
}
fn chip_parts_of(c: &BandChip, label: &str, f: ChipForm, alias_only: bool) -> Line {
    if c.token {
        return chip_parts("▲", Style::Warn, &format!("{label} {}", c.head));
    }
    let text = if alias_only { &c.short } else { &c.head };
    let head = if c.inactive {
        p(text.clone(), Style::Idle)
    } else {
        pl(text.clone())
    };
    let mut out: Line = vec![pl(label), pl(CHIP_SPACE), head];
    if !alias_only && f.countdown {
        if let Some(r) = &c.reset {
            out.push(pl(CHIP_SPACE));
            out.extend(chip_parts("◷", Style::Plain, r));
        }
    }
    if !alias_only && f.age {
        if let Some(a) = &c.age {
            out.push(p(" · ", Style::Dim));
            out.push(pl(a.clone()));
        }
    }
    out
}

/// The quota group, widest form first (section 6.1 drop order): everything; no `無資料 N`; no ages; no countdowns; no `provider:` prefixes
/// (where the alias is unique); whole chips from the tail until one is left; that one as `alias 62%`. The chips are CHIP_GAP apart.
pub fn band_forms(b: &Band) -> Vec<Line> {
    use std::collections::{BTreeMap, BTreeSet};
    let all = &b.chips;
    let mut keys_of: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for c in all {
        keys_of
            .entry(c.alias.as_str())
            .or_default()
            .insert(c.key.as_str());
    }
    let label = |c: &BandChip, bare: bool| -> String {
        if bare && keys_of.get(c.alias.as_str()).map_or(0, |s| s.len()) == 1 {
            c.alias.clone()
        } else {
            c.key.clone()
        }
    };
    let group = |chips: &[BandChip], f: ChipForm, with_no_data: bool, bare: bool| -> Line {
        let mut parts: Vec<Line> = chips
            .iter()
            .map(|c| chip_parts_of(c, &label(c, bare), f, false))
            .collect();
        if with_no_data && b.no_data > 0 {
            parts.push(vec![
                p(b.no_data_word.clone(), Style::Idle),
                pl(CHIP_SPACE),
                p(b.no_data.to_string(), Style::Idle),
            ]);
        }
        let mut out: Line = Vec::new();
        for (i, x) in parts.into_iter().enumerate() {
            if i > 0 {
                out.push(pl(CHIP_GAP));
            }
            out.extend(x);
        }
        out
    };
    let full = ChipForm {
        age: true,
        countdown: true,
    };
    let no_age = ChipForm {
        age: false,
        countdown: true,
    };
    let bare_f = ChipForm {
        age: false,
        countdown: false,
    };
    let mut forms: Vec<Line> = vec![
        group(all, full, true, false),
        group(all, full, false, false),
        group(all, no_age, false, false),
        group(all, bare_f, false, false),
        group(all, bare_f, false, true),
    ];
    for n in (1..all.len()).rev() {
        forms.push(group(&all[..n], bare_f, false, true));
    }
    if let Some(first) = all.first() {
        let unique = keys_of.get(first.alias.as_str()).map_or(0, |s| s.len()) == 1;
        let l = if unique {
            first.alias.clone()
        } else {
            first.key.clone()
        };
        forms.push(chip_parts_of(first, &l, bare_f, true));
    }
    let mut seen: Vec<String> = Vec::new();
    forms
        .into_iter()
        .filter(|f| {
            let t = plain(f);
            if t.is_empty() || seen.contains(&t) {
                return false;
            }
            seen.push(t);
            true
        })
        .collect()
}

// ---- `usage accounts` text ----
fn paint(s: &Part, color: bool) -> String {
    if !color {
        return s.text.clone();
    }
    match s.style {
        Style::Plain => s.text.clone(),
        Style::Dim => format!("\x1b[2m{}\x1b[0m", s.text),
        Style::Idle => format!("\x1b[90m{}\x1b[0m", s.text),
        Style::Bold => format!("\x1b[1m{}\x1b[0m", s.text),
        Style::Warn => format!("\x1b[33m{}\x1b[0m", s.text),
    }
}

/// The whole `usage accounts` screen: title, rule, blocks, footers. `cols` is the terminal width, already clamped to <= 100.
pub fn accounts_text(
    state: &QuotaState,
    entries: &[QEntry],
    cols: usize,
    now_s: i64,
    tr: &Tr,
    color: bool,
) -> String {
    let mut lines: Vec<Line> = Vec::new();
    let title = tr.q("title");
    let count = tr.t("quota.mirror_count", &[("n", &state.raw.len().to_string())]);
    let gap = cols.saturating_sub(
        UnicodeWidthStr::width(title.as_str()) + UnicodeWidthStr::width(count.as_str()),
    );
    if gap >= 2 {
        lines.push(vec![
            p(title, Style::Bold),
            pl(" ".repeat(gap)),
            p(count, Style::Dim),
        ]);
    } else {
        lines.push(vec![p(title, Style::Bold)]);
    }
    lines.push(vec![p("─".repeat(cols), Style::Dim)]);
    lines.extend(pane_lines(state, entries, cols, now_s, tr));
    let mut out = String::new();
    for l in lines.iter().map(|l| clip(l, cols)) {
        for s in &l {
            out.push_str(&paint(s, color));
        }
        out.push('\n');
    }
    out
}
