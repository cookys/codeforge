//! Catch-up formula for missed daemon ticks.
//!
//! Per `.claude/rpg-engine-spec.md`:
//! - Up to 240 missed ticks: full one-for-one catch-up (4 hours at 60s tick).
//! - Beyond 240: diminishing returns `effective = 240 + sqrt(missed - 240)`.
//!
//! Prevents 2-week-idle → instant-max-level exploits while still rewarding
//! re-engagement after a short break.

pub const CATCHUP_CAP: u64 = 240;

pub fn compute_effective_ticks(missed: u64) -> u64 {
    if missed <= CATCHUP_CAP {
        return missed;
    }
    let tail = missed - CATCHUP_CAP;
    CATCHUP_CAP + (tail as f64).sqrt() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_missed_yields_zero() {
        assert_eq!(compute_effective_ticks(0), 0);
    }

    #[test]
    fn below_cap_is_linear() {
        assert_eq!(compute_effective_ticks(1), 1);
        assert_eq!(compute_effective_ticks(100), 100);
        assert_eq!(compute_effective_ticks(239), 239);
    }

    #[test]
    fn at_cap_is_cap() {
        assert_eq!(compute_effective_ticks(CATCHUP_CAP), CATCHUP_CAP);
    }

    #[test]
    fn just_over_cap_adds_one() {
        // missed = 241 → tail = 1 → sqrt(1) = 1 → 240 + 1
        assert_eq!(compute_effective_ticks(241), 241);
    }

    #[test]
    fn two_weeks_idle_bounded() {
        // 14 days * 24h * 60min = 20160 ticks missed (1 tick / 60s)
        let effective = compute_effective_ticks(20160);
        // tail = 19920, sqrt ≈ 141.1 → 240 + 141 = 381
        assert!((380..=385).contains(&effective),
            "expected ~381 effective ticks for 2-week idle, got {}", effective);
    }

    #[test]
    fn u64_max_does_not_panic() {
        // sqrt of a huge number is still finite and fits in u64
        let effective = compute_effective_ticks(u64::MAX);
        assert!(effective < u64::MAX);
        assert!(effective > CATCHUP_CAP);
    }

    #[test]
    fn monotonically_non_decreasing() {
        let mut prev = 0u64;
        for m in [0, 1, 100, 240, 241, 1000, 10_000, 1_000_000, u64::MAX] {
            let e = compute_effective_ticks(m);
            assert!(e >= prev, "{}→{} broke monotonicity (prev {})", m, e, prev);
            prev = e;
        }
    }
}
