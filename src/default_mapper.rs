use serde_json::{Value, json};

use crate::{
    extract_trace_id, get_severity,
    google_logger::{LogContext, LogMapper},
    log_entry::{Labels, Resource},
};

#[derive(Clone, Default)]
pub struct DefaultLogMapper;

impl LogMapper for DefaultLogMapper {
    fn map(&self, context: LogContext, log_entry: Value) -> Value {
        let log_name = format!("projects/{}/logs/{}", context.project_id, context.log_label);

        let trace_id =
            extract_trace_id(&log_entry).unwrap_or_else(|| json!("trace_id is undefined"));

        json!({
            "log_name": log_name,
            "resource": Resource::new_global(context.project_id.to_string()),
            "severity": get_severity(&log_entry),
            "timestamp": log_entry
                .get("time")
                .cloned()
                .unwrap_or_else(|| json!(chrono::Utc::now().to_rfc3339())),
            "json_payload": log_entry,
            "trace": trace_id.clone(),
            "labels": Labels {
                context: context.log_label.to_string(),
                request_id: trace_id,
            },
        })
    }
}
