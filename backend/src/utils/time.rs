use chrono::{DateTime, Utc};

pub fn is_expired(expires_at: Option<DateTime<Utc>>) -> bool {
    match expires_at {
        Some(ts) => Utc::now() > ts,
        None => false,
    }
}
