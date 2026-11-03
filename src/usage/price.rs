//! Estimated API-equivalent cost (G36 plan, phase 3).
//!
//! Source: the bundled claude-api reference, "Current Models (cached: 2026-10-06)" table, and
//! `shared/prompt-caching.md` § Economics (cache writes 1.25× / 2× input; reads 0.1× input
//! except Opus 5.5 $0.20 and Fable 5.1 $0.25). Fast mode is 2× on Opus 5.5 / 5 / 4.8.
//! Every rate is written out per model (no rule-plus-exception code path). Subscription
//! plans are not billed this way: the figure is "API-equivalent", an estimate.

pub const PRICES_AS_OF: &str = "2026-10-06";

/// $ per million tokens, and the fast-mode multiplier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub fast_mult: f64,
}

const fn r(input: f64, output: f64, cw5m: f64, cw1h: f64, read: f64, fast: f64) -> Rates {
    Rates {
        input,
        output,
        cache_write_5m: cw5m,
        cache_write_1h: cw1h,
        cache_read: read,
        fast_mult: fast,
    }
}

/// One row per model id. Haiku 5.5 above 100 K prompt tokens costs more; v1 prices it flat.
const TABLE: &[(&str, Rates)] = &[
    ("claude-fable-5-1", r(10.0, 50.0, 12.5, 20.0, 0.25, 1.0)),
    ("claude-fable-5", r(10.0, 50.0, 12.5, 20.0, 1.0, 1.0)),
    ("claude-opus-5-5", r(4.0, 20.0, 5.0, 8.0, 0.20, 2.0)),
    ("claude-opus-5", r(5.0, 25.0, 6.25, 10.0, 0.50, 2.0)),
    ("claude-opus-4-8", r(5.0, 25.0, 6.25, 10.0, 0.50, 2.0)),
    ("claude-opus-4-7", r(5.0, 25.0, 6.25, 10.0, 0.50, 1.0)),
    ("claude-opus-4-6", r(5.0, 25.0, 6.25, 10.0, 0.50, 1.0)),
    ("claude-sonnet-5-5", r(2.0, 10.0, 2.5, 4.0, 0.20, 1.0)),
    ("claude-sonnet-5", r(2.0, 10.0, 2.5, 4.0, 0.20, 1.0)),
    ("claude-sonnet-4-6", r(3.0, 15.0, 3.75, 6.0, 0.30, 1.0)),
    ("claude-haiku-5-5", r(0.10, 0.50, 0.125, 0.20, 0.01, 1.0)),
    ("claude-haiku-4-5", r(1.0, 5.0, 1.25, 2.0, 0.10, 1.0)),
];

/// The id a price row is keyed by: a trailing `[…]` tag and a `-YYYYMMDD` snapshot suffix
/// are dropped (`claude-haiku-4-5-20251001` → `claude-haiku-4-5`).
fn normalize(model: &str) -> &str {
    let m = model.split('[').next().unwrap_or(model);
    match m.rsplit_once('-') {
        Some((head, tail)) if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => m,
    }
}

pub fn rates(model: &str) -> Option<&'static Rates> {
    let m = normalize(model);
    TABLE.iter().find(|(id, _)| *id == m).map(|(_, r)| r)
}

/// Token counts of one bucket.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
}

/// Estimated cost in dollars; `None` for a model without a price row (never 0).
pub fn cost(model: &str, fast: bool, t: &Tokens) -> Option<f64> {
    let r = rates(model)?;
    let m = if fast { r.fast_mult } else { 1.0 };
    let per = |n: u64, rate: f64| n as f64 * rate / 1_000_000.0;
    Some(
        m * (per(t.input, r.input)
            + per(t.output, r.output)
            + per(t.cache_write_5m, r.cache_write_5m)
            + per(t.cache_write_1h, r.cache_write_1h)
            + per(t.cache_read, r.cache_read)),
    )
}

/// A total over rows some of which may be unpriced: the sum of the priced ones and how many
/// were not. `(None, n)` when nothing was priced.
pub fn total(costs: impl IntoIterator<Item = Option<f64>>) -> (Option<f64>, usize) {
    let (mut sum, mut any, mut unpriced) = (0.0, false, 0usize);
    for c in costs {
        match c {
            Some(v) => {
                sum += v;
                any = true;
            }
            None => unpriced += 1,
        }
    }
    (any.then_some(sum), unpriced)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: u64 = 1_000_000;

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn known_models_and_the_formula() {
        let t = Tokens {
            input: M,
            output: M,
            cache_write_5m: M,
            cache_write_1h: M,
            cache_read: M,
        };
        // Opus 5.5: 4 + 20 + 5 + 8 + 0.20
        assert!(close(cost("claude-opus-5-5", false, &t), 37.2));
        // Sonnet 4.6: 3 + 15 + 3.75 + 6 + 0.30
        assert!(close(cost("claude-sonnet-4-6", false, &t), 28.05));
        // snapshot suffix and tag are dropped
        assert!(close(
            cost("claude-haiku-4-5-20251001", false, &t),
            1.0 + 5.0 + 1.25 + 2.0 + 0.1
        ));
        assert!(rates("claude-opus-5-5[1m]").is_some());
        // claude-opus-5 is not a prefix match for claude-opus-5-5 or the reverse
        assert_ne!(rates("claude-opus-5"), rates("claude-opus-5-5"));
    }

    #[test]
    fn the_two_cache_read_exceptions_are_pinned() {
        // the source table: Opus 5.5 $0.20 (0.05x), Fable 5.1 $0.25 (0.025x); the rest 0.1x input
        assert_eq!(rates("claude-opus-5-5").unwrap().cache_read, 0.20);
        assert_eq!(rates("claude-fable-5-1").unwrap().cache_read, 0.25);
        for (id, r) in TABLE {
            assert_eq!(r.cache_write_5m, r.input * 1.25, "{id}");
            assert_eq!(r.cache_write_1h, r.input * 2.0, "{id}");
            if !["claude-opus-5-5", "claude-fable-5-1"].contains(id) {
                assert!((r.cache_read - r.input * 0.1).abs() < 1e-12, "{id}");
            }
        }
    }

    #[test]
    fn fast_doubles_only_on_fast_tier_models() {
        let t = Tokens {
            input: M,
            ..Default::default()
        };
        assert!(close(cost("claude-opus-5-5", true, &t), 8.0));
        assert!(close(cost("claude-opus-4-8", true, &t), 10.0));
        // fast on a model without a fast tier: no multiplier
        assert!(close(cost("claude-sonnet-5-5", true, &t), 2.0));
        assert!(close(cost("claude-opus-4-7", true, &t), 5.0));
    }

    #[test]
    fn unknown_is_none_never_zero_and_totals_say_partial() {
        let t = Tokens {
            input: M,
            ..Default::default()
        };
        assert_eq!(cost("gpt-6.1-sol", false, &t), None);
        assert_eq!(cost("unknown", false, &t), None);
        assert_eq!(total([Some(1.5), None, Some(2.0), None]), (Some(3.5), 2));
        assert_eq!(total([None, None]), (None, 2));
        assert_eq!(total([]), (None, 0));
    }
}
