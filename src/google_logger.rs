use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::gauth::{GAuth, GAuthCredential, GAuthError};

pub trait LogMapper: Send + Sync + 'static {
    fn map(&self, logger: &GoogleLogger<Self>, entry: Value) -> serde_json::Value
    where
        Self: Sized;
}

#[derive(Debug)]
pub struct GoogleLogger<M: LogMapper> {
    pub log_label: Arc<str>,
    pub project_id: String,
    pub gauth: GAuth,
    pub http_client: Client,
    pub mapper: M,
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
    pub fn new(log_name: Arc<str>, credential_bytes: Arc<Vec<u8>>, mapper: M) -> GoogleLogger<M> {
        let service_account = GAuth::from_bytes(
            credential_bytes.as_ref(),
            &["https://www.googleapis.com/auth/logging.write"],
        );

        let project_id = GAuthCredential::from_bytes(credential_bytes.as_ref())
            .expect("Google Credential must be valid")
            .project_id;

        Self {
            log_label: log_name,
            project_id,
            gauth: service_account,
            http_client: Client::new(),
            mapper,
        }
    }

    pub async fn write_logs(&mut self, log_entry: Vec<Value>) -> Result<(), LoggerError> {
        let access_token = self.gauth.access_token().await?;
        let entries = log_entry
            .into_iter()
            .map(|v| self.mapper.map(&self, v))
            .collect::<Vec<_>>();

        // https://cloud.google.com/logging/docs/reference/v2/rest/v2/entries/write#response-body
        let maybe_response_error = self
            .http_client
            .post("https://logging.googleapis.com/v2/entries:write")
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

    pub fn log_label(&self) -> &str {
        &self.log_label
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }
}
