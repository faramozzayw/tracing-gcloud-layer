use super::google_logger::{GoogleLogger, LogMapper};
use crate::{GoogleWriterConfig, google_writer::GoogleWriterHandle};
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    sync::{RwLock, mpsc, oneshot},
    task::JoinHandle,
    time::sleep,
};

/// Runtime that manages a background task for batching and sending logs to Google Cloud.
///
/// `GoogleWriterRuntime` owns the background task, handles shutdown, and provides a
/// [`GoogleWriterHandle`] for synchronous log writing. It is generic over a type implementing [`LogMapper`],
/// which is responsible for mapping structured log entries to the format expected by Google Cloud Logging.
pub struct GoogleWriterRuntime<M: LogMapper> {
    handle: GoogleWriterHandle,
    shutdown_handle: Option<JoinHandle<()>>,
    shutdown_trigger: Option<oneshot::Sender<()>>,
    _marker: std::marker::PhantomData<M>,
}

impl<M: LogMapper + Send + Sync + 'static> GoogleWriterRuntime<M> {
    /// Creates a new `GoogleWriterRuntime` and spawns the background batching task.
    ///
    /// Logs are received via an unbounded channel, buffered, and flushed when:
    /// - the buffer reaches `config.max_batch` entries, or
    /// - `config.max_delay` elapses since the last flush.
    ///
    /// The background task also flushes any remaining logs on shutdown.
    pub fn new(google_logger: GoogleLogger<M>, config: GoogleWriterConfig) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let logger = Arc::new(RwLock::new(google_logger));
        let logger_clone = logger.clone();

        let handle_task = tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(config.max_batch);
            let mut shutdown_rx = shutdown_rx;

            loop {
                tokio::select! {
                    maybe_entry = rx.recv() => {
                        match maybe_entry {
                            Some(entry) => {
                                buffer.push(entry);
                                if buffer.len() >= config.max_batch {
                                    Self::flush_batch(&logger_clone, std::mem::take(&mut buffer)).await;
                                }
                            }
                            None => {
                                // Channel closed, flush remaining logs
                                if !buffer.is_empty() {
                                    Self::flush_batch(&logger_clone, buffer).await;
                                }
                                break;
                            }
                        }
                    }

                    _ = &mut shutdown_rx => {
                        // Flush buffer on shutdown
                        if !buffer.is_empty() {
                            Self::flush_batch(&logger_clone, buffer).await;
                        }
                        break;
                    }

                    _ = sleep(config.max_delay), if !buffer.is_empty() => {
                        // Flush due to max_delay timeout
                        Self::flush_batch(&logger_clone, std::mem::take(&mut buffer)).await;
                    }
                }
            }

            tracing::debug!("Background task shut down cleanly.");
        });

        Self {
            handle: GoogleWriterHandle { sender: tx },
            shutdown_handle: Some(handle_task),
            shutdown_trigger: Some(shutdown_tx),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns a handle that implements [`std::io::Write`] for sending log entries.
    ///
    /// This handle can be cloned and shared across threads for synchronous logging.
    pub fn writer(&self) -> GoogleWriterHandle {
        self.handle.clone()
    }

    /// Shuts down the background task, flushing any remaining logs.
    ///
    /// This method waits for the task to complete and logs any panics encountered during shutdown.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_trigger.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.shutdown_handle.take() {
            if let Err(err) = handle.await {
                tracing::error!("Shutdown task panicked: {:?}", err);
            }
        }
    }

    /// Flushes a batch of logs to Google Cloud Logging.
    ///
    /// Any errors encountered during the write are logged via `tracing::error!`.
    async fn flush_batch(logger: &Arc<RwLock<GoogleLogger<M>>>, batch: Vec<Value>) {
        let mut guard = logger.write().await;
        if let Err(err) = guard.write_logs(batch).await {
            tracing::error!("Failed to write log batch: {err}");
        }
    }
}
