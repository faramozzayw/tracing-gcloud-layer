use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::gauth::{GAuth, GAuthCredential, GAuthError};

const WRITE_URL: &str = "https://logging.googleapis.com/v2/entries:write";
const SCOPES: [&str; 1] = ["https://www.googleapis.com/auth/logging.write"];

#[derive(Debug, Clone)]
pub struct LogContext {
    pub log_label: Arc<str>,
    pub project_id: Arc<str>,
}

pub trait LogMapper: Send + Sync + 'static + Clone + Default {
    fn map(&self, context: LogContext, entry: Value) -> serde_json::Value
    where
        Self: Sized;
}

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
