use serde_json::{Value, json};

use crate::{
    extract_trace_id, get_severity,
    google_logger::{GoogleLogger, LogMapper},
    log_entry::{Labels, Resource},
};

pub struct DefaultLogMapper;

impl LogMapper for DefaultLogMapper {
    fn map(&self, logger: &GoogleLogger<Self>, log_entry: Value) -> Value {
        let project_id = logger.project_id();
        let log_label = logger.log_label();
        let log_name = format!("projects/{}/logs/{}", project_id, log_label);

        let trace_id =
            extract_trace_id(&log_entry).unwrap_or_else(|| json!("trace_id is undefined"));

        json!({
            "log_name": log_name,
            "resource": Resource::new_global(project_id.to_owned()),
            "severity": get_severity(&log_entry),
            "timestamp": log_entry
                .get("time")
                .cloned()
                .unwrap_or_else(|| json!(chrono::Utc::now().to_rfc3339())),
            "json_payload": log_entry,
            "trace": trace_id.clone(),
            "labels": Labels {
                context: log_label.to_owned(),
                request_id: trace_id,
            },
        })
    }
}
