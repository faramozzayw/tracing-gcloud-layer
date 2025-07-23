use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::gauth::{GAuth, GAuthCredential, GAuthError};

/// Google Cloud Logging API endpoint for writing log entries.
const WRITE_URL: &str = "https://logging.googleapis.com/v2/entries:write";
/// OAuth 2.0 scope for logging write access.
const SCOPES: [&str; 1] = ["https://www.googleapis.com/auth/logging.write"];

#[derive(Debug, Clone)]
pub struct LogContext {
    /// The log label associated with the logger (e.g., log name).
    pub log_label: Arc<str>,
    /// The GCP project ID where logs should be written.
    pub project_id: Arc<str>,
}

/// Trait for mapping a raw JSON log entry to a structured format compatible with Google Cloud Logging.
///
/// You can implement this to transform log data (e.g., enrich with labels or restructure).
pub trait LogMapper: Send + Sync + Clone + Default + 'static {
    /// Converts a raw log entry into a structured JSON value using context information.
    fn map(&self, context: LogContext, entry: Value) -> serde_json::Value
    where
        Self: Sized;
}

/// A logger that writes entries to Google Cloud Logging using the [entries.write](https://cloud.google.com/logging/docs/reference/v2/rest/v2/entries/write) API.
#[derive(Debug, Clone)]
pub struct GoogleLogger<M: LogMapper> {
    log_context: LogContext,
    gauth: GAuth,
    http_client: Client,
    mapper: M,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub error: ResponseErrorInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseErrorInner {
    pub code: Option<i64>,
    pub message: String,
    pub status: String,
}

#[derive(Error, Debug)]
pub enum LoggerError {
    #[error("ReqwestError: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Google error: {:?}", .0)]
    Response(ResponseErrorInner),
    #[error("Service Account: {}", .0)]
    GAuth(#[from] GAuthError),
}

impl<M: LogMapper> GoogleLogger<M> {
    /// Creates a new `GoogleLogger` with the given log label, service account credentials, and log mapper.
    pub fn new(
        log_label: Arc<str>,
        credential_bytes: impl AsRef<[u8]>,
        mapper: M,
    ) -> Result<GoogleLogger<M>, LoggerError> {
        let credential_bytes = credential_bytes.as_ref();
        let service_account = GAuth::from_bytes(credential_bytes, &SCOPES);
        let project_id = GAuthCredential::from_bytes(credential_bytes)
            .map_err(|e| LoggerError::GAuth(GAuthError::SerdeJson(e)))?
            .project_id;

        let project_id = Arc::from(project_id);

        Ok(Self {
            log_context: LogContext {
                log_label,
                project_id,
            },
            gauth: service_account,
            http_client: Client::new(),
            mapper,
        })
    }

    /// Sends a batch of log entries to Google Cloud Logging.
    ///
    /// Each entry is passed through the configured `LogMapper` before being sent.
    pub async fn write_logs(&mut self, log_entry: Vec<Value>) -> Result<(), LoggerError> {
        let access_token = self.gauth.access_token().await?;
        let entries = log_entry
            .into_iter()
            .map(|v| self.mapper.map(self.context(), v))
            .collect::<Vec<_>>();

        // https://cloud.google.com/logging/docs/reference/v2/rest/v2/entries/write#response-body
        let maybe_response_error = self
            .http_client
            .post(WRITE_URL)
            .header("Content-Type", "application/json")
            .header("Authorization", access_token)
            .json(&json!({
                "entries": entries,
            }))
            .send()
            .await?
            .json::<ResponseError>()
            .await
            .ok();

        if let Some(ResponseError { error }) = maybe_response_error {
            return Err(LoggerError::Response(error));
        }

        Ok(())
    }

    #[inline]
    pub fn context(&self) -> LogContext {
        self.log_context.clone()
    }
}
