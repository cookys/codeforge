//! HTTP transport + retry policy (ship spec §7, source-contract §9.1).
//!
//! Core rule: a single payload always reuses the same ship_id; retry never mints a
//! new ULID (§7.1). Backoff schedule: 1s → 5s → 30s, then fail. 4xx never retries
//! (except 409 / duplicate which count as success).

use serde::Serialize;
use std::time::Duration;

/// Classification of one POST attempt's outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum AttemptOutcome {
    /// 2xx accepted, or duplicate (409 / `{status:"duplicate"}`) — terminal success.
    Success,
    /// 4xx (not 409) — terminal failure, do NOT retry, do NOT queue (bad payload).
    BadRequest(String),
    /// 5xx or network error — retryable.
    Retryable(String),
}

/// Result of the full retry loop.
#[derive(Debug, Clone, PartialEq)]
pub enum SendResult {
    /// Succeeded (possibly after retries).
    Ok,
    /// 4xx — caller should report error, NOT write ship-failed/ (§7.2).
    BadRequest(String),
    /// Exhausted retries on a retryable error — caller should write ship-failed/ (§7.2).
    Exhausted(String),
}

/// Default backoff schedule (§7.2): waits BETWEEN attempts. N waits → N+1 attempts.
pub const BACKOFF_SCHEDULE: &[Duration] = &[
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(30),
];

/// Drive the §7.2 backoff retry loop around an async attempt.
///
/// `attempt` is awaited up to `schedule.len() + 1` times; between retryable failures we
/// `tokio::time::sleep` the next backoff duration. This is the **single source of truth**
/// for the backoff policy — both the live ship path (`send_envelope`) and the failed-queue
/// flush (`flush_failed_queue`) drive it, instead of inlining the schedule separately.
/// Tests pause tokio's clock (`start_paused`) so they exercise the decision logic without
/// actually waiting 1s/5s/30s.
pub async fn run_with_retry_async<A, Fut>(schedule: &[Duration], mut attempt: A) -> SendResult
where
    A: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = AttemptOutcome>,
{
    let max_attempts = schedule.len() + 1;
    let mut last_err = String::new();
    for i in 0..max_attempts {
        match attempt(i).await {
            AttemptOutcome::Success => return SendResult::Ok,
            AttemptOutcome::BadRequest(msg) => return SendResult::BadRequest(msg),
            AttemptOutcome::Retryable(msg) => {
                last_err = msg;
                if i < schedule.len() {
                    tokio::time::sleep(schedule[i]).await;
                }
            }
        }
    }
    SendResult::Exhausted(last_err)
}

/// Single-attempt mode for `--no-hook` (§7.3): one try, no backoff.
pub fn run_single<A>(mut attempt: A) -> SendResult
where
    A: FnMut(usize) -> AttemptOutcome,
{
    match attempt(0) {
        AttemptOutcome::Success => SendResult::Ok,
        AttemptOutcome::BadRequest(msg) => SendResult::BadRequest(msg),
        AttemptOutcome::Retryable(msg) => SendResult::Exhausted(msg),
    }
}

/// Map an HTTP status + optional response body to an AttemptOutcome (§7.2).
pub fn classify(status: u16, body_status_field: Option<&str>) -> AttemptOutcome {
    // duplicate signalled either by 409 or by a `{ "status": "duplicate" }` body.
    if status == 409 || body_status_field == Some("duplicate") {
        return AttemptOutcome::Success;
    }
    match status {
        200..=299 => AttemptOutcome::Success,
        400..=499 => AttemptOutcome::BadRequest(format!("HTTP {status}")),
        _ => AttemptOutcome::Retryable(format!("HTTP {status}")),
    }
}

/// Perform a single POST attempt and classify the outcome (live network).
/// Network/timeout errors map to Retryable.
pub async fn post_attempt<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    body: &T,
) -> AttemptOutcome {
    let mut req = client.post(url).json(body);
    if let Some(tok) = token {
        req = req.header("Authorization", format!("Bearer {tok}"));
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body_status = resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(String::from));
            classify(status, body_status.as_deref())
        }
        Err(e) => AttemptOutcome::Retryable(format!("network: {e}")),
    }
}

/// Build a reqwest client with a sane timeout for ship/cite calls.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn classify_2xx_success() {
        assert_eq!(classify(200, Some("accepted")), AttemptOutcome::Success);
        assert_eq!(classify(201, None), AttemptOutcome::Success);
    }

    #[test]
    fn classify_duplicate_is_success() {
        assert_eq!(classify(200, Some("duplicate")), AttemptOutcome::Success);
        assert_eq!(classify(409, None), AttemptOutcome::Success);
    }

    #[test]
    fn classify_4xx_bad_request() {
        assert!(matches!(classify(400, None), AttemptOutcome::BadRequest(_)));
        assert!(matches!(classify(404, None), AttemptOutcome::BadRequest(_)));
    }

    #[test]
    fn classify_5xx_retryable() {
        assert!(matches!(classify(500, None), AttemptOutcome::Retryable(_)));
        assert!(matches!(classify(503, None), AttemptOutcome::Retryable(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn async_retry_succeeds_first_try() {
        let attempts = Cell::new(0usize);
        let res = run_with_retry_async(BACKOFF_SCHEDULE, |_| {
            attempts.set(attempts.get() + 1);
            async { AttemptOutcome::Success }
        })
        .await;
        assert_eq!(res, SendResult::Ok);
        assert_eq!(attempts.get(), 1); // success on first try, no retry
    }

    #[tokio::test(start_paused = true)]
    async fn async_retry_succeeds_on_third_attempt() {
        let attempts = Cell::new(0usize);
        let res = run_with_retry_async(BACKOFF_SCHEDULE, |i| {
            attempts.set(attempts.get() + 1);
            async move {
                if i < 2 {
                    AttemptOutcome::Retryable("5xx".to_string())
                } else {
                    AttemptOutcome::Success
                }
            }
        })
        .await;
        assert_eq!(res, SendResult::Ok);
        assert_eq!(attempts.get(), 3); // failed twice (slept 1s, 5s), succeeded on 3rd
    }

    #[tokio::test(start_paused = true)]
    async fn async_retry_exhausts_after_four_attempts() {
        let attempts = Cell::new(0usize);
        let res = run_with_retry_async(BACKOFF_SCHEDULE, |_| {
            attempts.set(attempts.get() + 1);
            async { AttemptOutcome::Retryable("network".to_string()) }
        })
        .await;
        assert!(matches!(res, SendResult::Exhausted(_)));
        assert_eq!(attempts.get(), 4); // schedule.len()=3 → 4 attempts then exhaust
    }

    #[tokio::test(start_paused = true)]
    async fn async_retry_4xx_stops_immediately() {
        let attempts = Cell::new(0usize);
        let res = run_with_retry_async(BACKOFF_SCHEDULE, |_| {
            attempts.set(attempts.get() + 1);
            async { AttemptOutcome::BadRequest("400".to_string()) }
        })
        .await;
        assert!(matches!(res, SendResult::BadRequest(_)));
        assert_eq!(attempts.get(), 1); // no retry on 4xx
    }

    #[test]
    fn single_attempt_retryable_becomes_exhausted() {
        let res = run_single(|_| AttemptOutcome::Retryable("net".to_string()));
        assert!(matches!(res, SendResult::Exhausted(_)));
    }

    #[test]
    fn single_attempt_success() {
        assert_eq!(run_single(|_| AttemptOutcome::Success), SendResult::Ok);
    }

    #[test]
    fn single_attempt_bad_request() {
        let res = run_single(|_| AttemptOutcome::BadRequest("400".to_string()));
        assert!(matches!(res, SendResult::BadRequest(_)));
    }
}
