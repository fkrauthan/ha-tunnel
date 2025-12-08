use std::time::{SystemTime, UNIX_EPOCH};

pub mod error;
pub mod tunnel;

pub fn now_as_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
