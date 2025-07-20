use std::time::{SystemTime, SystemTimeError};

use serde_json::Value;

#[inline]
pub fn get_severity(log_entry: &Value) -> Value {
    log_entry
        .get("severity")
        .cloned()
        .unwrap_or_else(|| "DEFAULT".into())
}

pub fn extract_trace_id(log_entry: &Value) -> Option<Value> {
    log_entry
        .get("span")
        .and_then(|v| v.get("trace_id"))
        .cloned()
}

#[inline]
pub fn timestamp() -> Result<u64, SystemTimeError> {
    Ok(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs())
}
