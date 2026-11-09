//! The operator's weekly reset rule for a note (`accounts.toml` `[resets]`): `<dow>@<HH:MM><±HH:MM>`,
//! e.g. `fri@17:00+08:00` = every Friday 17:00 at UTC+8. Operator-provided, so low confidence: it
//! is shown next to the note and never replaces a real quota window's reset.

/// A parsed rule: weekday (0 = Monday), time of day and UTC offset, all in seconds where timed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    dow: i64,
    tod_s: i64,
    off_s: i64,
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
/// The rule form, for error messages.
pub const EXAMPLE: &str = "fri@17:00+08:00";

fn two(s: &str) -> Option<i64> {
    (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().unwrap())
}

/// `HH:MM` as (hours, minutes), hours <= 23, minutes <= 59.
fn hm(s: &str) -> Option<(i64, i64)> {
    let (h, m) = s.split_once(':')?;
    let (h, m) = (two(h)?, two(m)?);
    (h <= 23 && m <= 59).then_some((h, m))
}

/// Parse `<dow>@<HH:MM><±HH:MM>`; `None` when it does not match exactly (lowercase day, explicit sign).
pub fn parse(s: &str) -> Option<Rule> {
    let (day, rest) = s.split_once('@')?;
    let dow = DAYS.iter().position(|d| *d == day)? as i64;
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
    Some(Rule {
        dow,
        tod_s: h * 3600 + m * 60,
        off_s: sign * (oh * 3600 + om * 60),
    })
}

/// Whether `s` is a valid rule.
pub fn valid(s: &str) -> bool {
    parse(s).is_some()
}

impl Rule {
    /// The first occurrence strictly after `now_s` (epoch seconds).
    pub fn next_after(&self, now_s: i64) -> i64 {
        const DAY: i64 = 86_400;
        let day = (now_s + self.off_s).div_euclid(DAY);
        // 1970-01-01 (day 0) was a Thursday = 3
        let today = (day + 3).rem_euclid(7);
        let ahead = (self.dow - today).rem_euclid(7);
        let t = (day + ahead) * DAY + self.tod_s - self.off_s;
        if t > now_s {
            t
        } else {
            t + 7 * DAY
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2025-11-09 00:00 UTC, a Sunday
    const MON_0000_UTC: i64 = 1_762_646_400; // 2025-11-09 is a Sunday

    fn next(rule: &str, now: i64) -> i64 {
        parse(rule).unwrap().next_after(now)
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
}
