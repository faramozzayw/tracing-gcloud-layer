use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub struct LogEntry {
    #[serde(rename = "logName")]
    pub log_name: String,
    pub resource: Resource,
    pub severity: Value,
    #[serde(rename = "jsonPayload")]
    pub json_payload: Value,
    pub timestamp: Value,
    pub labels: Labels,
    pub trace: Value,
}

#[derive(Serialize)]
pub struct Labels {
    pub context: String,
    #[serde(rename = "requestId")]
    pub request_id: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLabels {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub labels: ResourceLabels,
    #[serde(rename = "type")]
    pub resource_type: String,
}

impl Resource {
    pub fn new_global(project_id: String) -> Self {
        Resource {
            labels: ResourceLabels { project_id },
            resource_type: "global".to_owned(),
        }
    }
}
