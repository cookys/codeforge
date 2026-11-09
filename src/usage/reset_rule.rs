//! The operator's reset rule for a note (`accounts.toml` `[resets]`), one of three forms:
//! - weekly `<dow>@<HH:MM><±HH:MM>`, e.g. `fri@17:00+08:00` = every Friday 17:00 at UTC+8;
//! - monthly `<day>@<HH:MM><±HH:MM>`, day 1..=31 without a leading zero, e.g. `17@00:00+08:00`;
//!   a month without that day resets on its last day;
//! - one-shot RFC 3339 `YYYY-MM-DDTHH:MM:SS<±HH:MM|Z>`, e.g. `2026-11-21T00:00:00+08:00`; once
//!   passed it has no next reset.
//!
//! Operator-provided, so low confidence: shown next to the note and never replaces a real quota
//! window's reset.

use chrono::{Datelike, NaiveDate};

/// A parsed rule; times in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rule {
    /// weekday (0 = Monday), time of day, UTC offset
    Weekly { dow: i64, tod_s: i64, off_s: i64 },
    /// day of month (1..=31), time of day, UTC offset
    Monthly { dom: u32, tod_s: i64, off_s: i64 },
    /// one instant (epoch seconds)
    At { at_s: i64 },
}

/// What kind of rule it is, for labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Weekly,
    Monthly,
    At,
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
/// Example forms, for error messages.
pub const EXAMPLE: &str = "fri@17:00+08:00";
pub const EXAMPLE_MONTHLY: &str = "17@00:00+08:00";
pub const EXAMPLE_AT: &str = "2026-11-21T00:00:00+08:00";

fn digits(s: &str, n: usize) -> Option<i64> {
    (s.len() == n && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().unwrap())
}

/// `HH:MM` as (hours, minutes), hours <= 23, minutes <= 59.
fn hm(s: &str) -> Option<(i64, i64)> {
    let (h, m) = s.split_once(':')?;
    let (h, m) = (digits(h, 2)?, digits(m, 2)?);
    (h <= 23 && m <= 59).then_some((h, m))
}

/// `<HH:MM><±HH:MM>` as (time of day, offset) seconds.
fn time_off(rest: &str) -> Option<(i64, i64)> {
    if rest.len() != 11 || !rest.is_char_boundary(5) {
        return None;
    }
    let (time, off) = rest.split_at(5);
    let (h, m) = hm(time)?;
    let sign = match off.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (oh, om) = hm(&off[1..])?;
    Some((h * 3600 + m * 60, sign * (oh * 3600 + om * 60)))
}

/// `YYYY-MM-DDTHH:MM:SS<±HH:MM|Z>` (uppercase `T`, no fraction, seconds <= 59), a real calendar
/// instant.
fn instant(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let shape = s.len() >= 20
        && s.is_ascii()
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && [0..4, 5..7, 8..10, 11..13, 14..16, 17..19]
            .iter()
            .all(|r| b[r.clone()].iter().all(u8::is_ascii_digit))
        // no leap second: chrono takes `:60`, forge-stats (and the spec) do not
        && b[17] <= b'5';
    if !shape {
        return None;
    }
    let tz = &s[19..];
    let tz_ok = tz == "Z" || (matches!(tz.as_bytes()[0], b'+' | b'-') && hm(&tz[1..]).is_some());
    if !tz_ok {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.timestamp())
}

/// Parse a rule in one of the three forms; `None` when it does not match one exactly.
pub fn parse(s: &str) -> Option<Rule> {
    if let Some(at_s) = instant(s) {
        return Some(Rule::At { at_s });
    }
    let (day, rest) = s.split_once('@')?;
    let (tod_s, off_s) = time_off(rest)?;
    if let Some(dow) = DAYS.iter().position(|d| *d == day) {
        return Some(Rule::Weekly {
            dow: dow as i64,
            tod_s,
            off_s,
        });
    }
    let dom_ok = matches!(day.len(), 1 | 2)
        && day.bytes().all(|b| b.is_ascii_digit())
        && !day.starts_with('0');
    let dom: u32 = if dom_ok {
        day.parse().ok()?
    } else {
        return None;
    };
    (1..=31)
        .contains(&dom)
        .then_some(Rule::Monthly { dom, tod_s, off_s })
}

/// Whether `s` is a valid rule of any form.
pub fn valid(s: &str) -> bool {
    parse(s).is_some()
}

/// Whether `s` is a valid rule of `kind`.
pub fn valid_kind(s: &str, kind: Kind) -> bool {
    parse(s).is_some_and(|r| r.kind() == kind)
}

fn last_day(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map_or(28, |d| d.day())
}

impl Rule {
    pub fn kind(&self) -> Kind {
        match self {
            Rule::Weekly { .. } => Kind::Weekly,
            Rule::Monthly { .. } => Kind::Monthly,
            Rule::At { .. } => Kind::At,
        }
    }

    /// The first occurrence strictly after `now_s` (epoch seconds); `None` for a one-shot rule
    /// that is not after `now_s`.
    pub fn next_after(&self, now_s: i64) -> Option<i64> {
        const DAY: i64 = 86_400;
        match *self {
            Rule::Weekly { dow, tod_s, off_s } => {
                let day = (now_s + off_s).div_euclid(DAY);
                // 1970-01-01 (day 0) was a Thursday = 3
                let today = (day + 3).rem_euclid(7);
                let ahead = (dow - today).rem_euclid(7);
                let t = (day + ahead) * DAY + tod_s - off_s;
                Some(if t > now_s { t } else { t + 7 * DAY })
            }
            Rule::Monthly { dom, tod_s, off_s } => {
                let local = chrono::DateTime::from_timestamp(now_s + off_s, 0)?.date_naive();
                let (mut y, mut m) = (local.year(), local.month());
                // this month's reset, else next month's (always later than now)
                for _ in 0..2 {
                    let d = NaiveDate::from_ymd_opt(y, m, dom.min(last_day(y, m)))?;
                    let t = d.and_hms_opt(0, 0, 0)?.and_utc().timestamp() + tod_s - off_s;
                    if t > now_s {
                        return Some(t);
                    }
                    (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                }
                None
            }
            Rule::At { at_s } => (at_s > now_s).then_some(at_s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2025-11-09 00:00 UTC, a Sunday
    const MON_0000_UTC: i64 = 1_762_646_400; // 2025-11-09 is a Sunday

    fn next(rule: &str, now: i64) -> i64 {
        parse(rule).unwrap().next_after(now).unwrap()
    }

    #[test]
    fn anchors_are_right() {
        // 2025-11-10 00:00 UTC is a Monday
        let mon = MON_0000_UTC + 86_400;
        assert_eq!((mon.div_euclid(86_400) + 3).rem_euclid(7), 0);
    }

    #[test]
    fn same_day_before_and_after_the_time() {
        let mon = MON_0000_UTC + 86_400;
        // Monday 08:00 UTC; rule mon 17:00 UTC: later today
        assert_eq!(next("mon@17:00+00:00", mon + 8 * 3600), mon + 17 * 3600);
        // Monday 18:00: next week
        assert_eq!(
            next("mon@17:00+00:00", mon + 18 * 3600),
            mon + 7 * 86_400 + 17 * 3600
        );
        // exactly at the time is not "after now"
        assert_eq!(
            next("mon@17:00+00:00", mon + 17 * 3600),
            mon + 7 * 86_400 + 17 * 3600
        );
    }

    #[test]
    fn week_wrap_and_other_days() {
        let mon = MON_0000_UTC + 86_400;
        // Sunday 10:00 -> Monday 09:00 the next day
        let sun = mon + 6 * 86_400;
        assert_eq!(
            next("mon@09:00+00:00", sun + 10 * 3600),
            sun + 86_400 + 9 * 3600
        );
        // Friday rule from Monday noon
        assert_eq!(
            next("fri@17:00+00:00", mon + 12 * 3600),
            mon + 4 * 86_400 + 17 * 3600
        );
        // Thursday rule on a Friday: 6 days ahead
        assert_eq!(
            next("thu@00:00+00:00", mon + 4 * 86_400 + 1),
            mon + 10 * 86_400
        );
    }

    #[test]
    fn offsets_move_the_day_and_the_hour() {
        let mon = MON_0000_UTC + 86_400;
        // fri 17:00 at +08:00 = Friday 09:00 UTC
        assert_eq!(next("fri@17:00+08:00", mon), mon + 4 * 86_400 + 9 * 3600);
        // Sunday 20:00 UTC is already Monday 04:00 at +08:00: mon 03:00+08:00 has passed, next week
        let sun = mon + 6 * 86_400;
        assert_eq!(
            next("mon@03:00+08:00", sun + 20 * 3600),
            sun + 7 * 86_400 + 19 * 3600
        );
        // a negative offset: thu 02:00-05:00 = Thursday 07:00 UTC
        assert_eq!(next("thu@02:00-05:00", mon), mon + 3 * 86_400 + 7 * 3600);
        // a half-hour offset
        assert_eq!(next("tue@10:00+05:30", mon), mon + 86_400 + 4 * 3600 + 1800);
    }

    #[test]
    fn only_the_exact_form_parses() {
        for ok in ["fri@17:00+08:00", "mon@00:00-00:00", "sun@23:59+14:00"] {
            assert!(valid(ok), "{ok}");
        }
        for bad in [
            "",
            "fri",
            "fri@17:00",
            "Fri@17:00+08:00",
            "friday@17:00+08:00",
            "fri@7:00+08:00",
            "fri@17:00 +08:00",
            "fri@17:00+0800",
            "fri@24:00+08:00",
            "fri@17:60+08:00",
            "fri@17:00+08:60",
            "fri@17:00z",
            "fri@17:00+8:00",
            "fri@１７:00+08:00",
            "fri@17:00+08:00 ",
            "fri@17:00+08:0é",
        ] {
            assert!(!valid(bad), "{bad:?}");
        }
    }

    // 2026-11-09 04:00 UTC (12:00 at +08:00), a Monday
    const NOV9: i64 = 1_794_196_800;

    #[test]
    fn monthly_this_month_next_month_and_the_last_day() {
        // 17th 00:00 +08:00 = 16th 16:00 UTC; 2026-11-16 16:00 UTC is 7 days 12 h after NOV9
        assert_eq!(next("17@00:00+08:00", NOV9), NOV9 + 7 * 86_400 + 12 * 3600);
        // the 9th 10:00 +08:00 has passed (it is 12:00): next is 2026-12-09 10:00 +08:00
        assert_eq!(next("9@10:00+08:00", NOV9), NOV9 + 30 * 86_400 - 2 * 3600);
        // the 31st in November is the 30th; 2026-11-30 00:00 UTC
        let nov30 = NOV9 + 21 * 86_400 - 4 * 3600;
        assert_eq!(next("31@00:00+00:00", NOV9), nov30);
        // and from just after it, December's 31st
        assert_eq!(next("31@00:00+00:00", nov30), nov30 + 31 * 86_400);
        // February 2027 has 28 days: from 2027-02-01 the 30th is 2027-02-28
        let feb1 = 1_801_440_000; // 2027-02-01 00:00 UTC
        assert_eq!(next("30@00:00+00:00", feb1), feb1 + 27 * 86_400);
        // December rolls to January of the next year: 2026-12-20 -> 2027-01-05
        let dec20 = NOV9 + 41 * 86_400 - 4 * 3600;
        assert_eq!(next("5@00:00+00:00", dec20), dec20 + 16 * 86_400);
        // the offset decides the local day: 2026-11-30 20:00 UTC is already 12-01 04:00 at +08:00,
        // so 1@03:00+08:00 has passed and the next is 2027-01-01 03:00 +08:00
        assert_eq!(
            next("1@03:00+08:00", nov30 + 20 * 3600),
            nov30 + 31 * 86_400 + 19 * 3600
        );
    }

    #[test]
    fn a_one_shot_counts_down_until_it_passes() {
        let r = parse("2026-11-21T00:00:00+08:00").unwrap();
        let at = NOV9 + 11 * 86_400 + 12 * 3600;
        assert_eq!(r.next_after(NOV9), Some(at));
        assert_eq!(r.next_after(at - 1), Some(at));
        assert_eq!(r.next_after(at), None);
        assert_eq!(
            parse("2026-11-20T16:00:00Z").unwrap().next_after(NOV9),
            Some(at)
        );
    }

    #[test]
    fn each_form_has_its_kind_and_the_monthly_and_one_shot_forms_are_exact() {
        assert_eq!(parse("fri@17:00+08:00").unwrap().kind(), Kind::Weekly);
        assert_eq!(parse("17@00:00+08:00").unwrap().kind(), Kind::Monthly);
        assert_eq!(parse(EXAMPLE_AT).unwrap().kind(), Kind::At);
        assert!(valid_kind(EXAMPLE_MONTHLY, Kind::Monthly));
        assert!(!valid_kind(EXAMPLE_MONTHLY, Kind::Weekly));
        assert!(!valid_kind(EXAMPLE, Kind::At));
        for ok in ["1@00:00+00:00", "31@23:59-05:00", "2026-02-28T23:59:59Z"] {
            assert!(valid(ok), "{ok}");
        }
        for bad in [
            "0@00:00+08:00",
            "32@00:00+08:00",
            "01@00:00+08:00",
            "100@00:00+08:00",
            "17@00:00",
            "17@24:00+08:00",
            "+1@00:00+08:00",
            "2026-11-21",
            "2026-11-21T00:00+08:00",
            "2026-11-21t00:00:00+08:00",
            "2026-11-21 00:00:00+08:00",
            "2026-11-21T00:00:00.5+08:00",
            "2026-11-21T00:00:00",
            "2026-11-21T00:00:00z",
            "2026-11-21T00:00:00+0800",
            "2026-02-30T00:00:00Z",
            "2026-11-21T24:00:00Z",
            "2026-11-21T00:00:60Z",
            "2026-12-31T23:59:60Z",
            "２026-11-21T00:00:00Z",
        ] {
            assert!(!valid(bad), "{bad:?}");
        }
    }
}
