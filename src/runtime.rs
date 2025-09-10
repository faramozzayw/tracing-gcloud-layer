use super::google_logger::{GoogleLogger, LogMapper};
use crate::{GoogleWriterConfig, google_writer::GoogleWriterHandle};
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    sync::{RwLock, mpsc, oneshot},
    task::JoinHandle,
    time::sleep,
};

/// Messages sent from the runtime to the background worker
enum ControlMessage {
    Flush(oneshot::Sender<()>),
    Shutdown(oneshot::Sender<()>),
}

/// Runtime that manages a background task for batching and sending logs to Google Cloud.
///
/// `GoogleWriterRuntime` owns the background task, handles shutdown, and provides a
/// [`GoogleWriterHandle`] for synchronous log writing. It is generic over a type implementing [`LogMapper`],
/// which is responsible for mapping structured log entries to the format expected by Google Cloud Logging.
#[derive(Debug)]
pub struct GoogleWriterRuntime<M: LogMapper> {
    handle: GoogleWriterHandle,
    control_tx: mpsc::Sender<ControlMessage>,
    shutdown_handle: Option<JoinHandle<()>>,
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
        let (control_tx, mut control_rx) = mpsc::channel::<ControlMessage>(8);

        let logger = Arc::new(RwLock::new(google_logger));
        let logger_clone = logger.clone();

        let handle_task = tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(config.max_batch);

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
                                // Sender dropped, flush remaining logs
                                if !buffer.is_empty() {
                                    Self::flush_batch(&logger_clone, buffer).await;
                                }
                                break;
                            }
                        }
                    }

                    Some(ctrl) = control_rx.recv() => {
                        match ctrl {
                            ControlMessage::Flush(reply) => {
                                if !buffer.is_empty() {
                                    Self::flush_batch(&logger_clone, std::mem::take(&mut buffer)).await;
                                }
                                let _ = reply.send(());
                            }
                            ControlMessage::Shutdown(reply) => {
                                if !buffer.is_empty() {
                                    Self::flush_batch(&logger_clone, buffer).await;
                                }
                                let _ = reply.send(());
                                break;
                            }
                        }
                    }

                    _ = sleep(config.max_delay), if !buffer.is_empty() => {
                        Self::flush_batch(&logger_clone, std::mem::take(&mut buffer)).await;
                    }
                }
            }

            tracing::debug!("Background task shut down cleanly.");
        });

        Self {
            handle: GoogleWriterHandle { sender: tx },
            control_tx,
            shutdown_handle: Some(handle_task),
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns a handle that implements [`std::io::Write`] for sending log entries.
    ///
    /// This handle can be cloned and shared across threads for synchronous logging.
    pub fn writer(&self) -> GoogleWriterHandle {
        self.handle.clone()
    }

    /// Flushes all buffered logs **and waits** until the background task completes the flush.
    pub async fn flush_and_wait(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self.control_tx.send(ControlMessage::Flush(tx)).await;
        let _ = rx.await;
    }

    /// Shuts down the background task, flushing any remaining logs.
    ///
    /// This method waits for the task to complete and logs any panics encountered during shutdown.
    pub async fn shutdown(mut self) {
        if let Some(handle) = self.shutdown_handle.take() {
            let (tx, rx) = oneshot::channel();
            let _ = self.control_tx.send(ControlMessage::Shutdown(tx)).await;
            let _ = rx.await;
            if let Err(err) = handle.await {
                tracing::error!("Shutdown task panicked: {:?}", err);
            }
        }
    }

    /// Flushes a batch of logs to Google Cloud Logging.
    async fn flush_batch(logger: &Arc<RwLock<GoogleLogger<M>>>, batch: Vec<Value>) {
        let mut guard = logger.write().await;
        if let Err(err) = guard.write_logs(batch).await {
            tracing::error!("Failed to write log batch: {err}");
        }
    }
}
