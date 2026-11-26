//! One attempt to read a live quota, and the bounded file read both callers share.
//!
//! [`Fetch`] never carries a bearer: [`Fetch::Body`] is untrusted response JSON, and the other
//! variants carry at most a closed [`AuthState`] verdict. [`read_capped`] drops every I/O and
//! parse error instead of formatting it, because the file it opens may be a credential.

use serde::de::DeserializeOwned;
use serde_json::Value;
use std::io::Read;
use std::path::Path;

use super::quota::AuthState;

/// What one attempt to read the live quota produced. Never carries the bearer.
pub enum Fetch {
    /// A JSON body. Untrusted; only the caller's parser decides what is kept.
    Body(Value),
    /// The credential is not accepted: `state` is the verdict this attempt reached (an expired
    /// credential, or a 401/403 from the service). Do not refresh.
    Denied(AuthState),
    /// Off switch, transport error, or a credential file that cannot be used: no verdict, the
    /// state is [`AuthState::Unknown`].
    Failed,
}

/// The bytes of a regular file of at most `max` bytes, opened without following a symlink.
/// Missing, a link, a directory, or over the cap is `None`.
pub(crate) fn read_capped(path: &Path, max: u64) -> Option<Vec<u8>> {
    let file = super::quota::open_no_follow(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() || meta.len() > max {
        return None;
    }
    let mut buf = Vec::new();
    file.take(max + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > max {
        return None;
    }
    Some(buf)
}

/// [`read_capped`] parsed as `T`. A body that is not `T` is `None`: the error is dropped, never
/// formatted, because the file may hold a credential.
pub(crate) fn read_capped_json<T: DeserializeOwned>(path: &Path, max: u64) -> Option<T> {
    serde_json::from_slice(&read_capped(path, max)?).ok()
}
