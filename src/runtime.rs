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
#[derive(Debug)]
pub struct GoogleWriterRuntime<M: LogMapper> {
    handle: GoogleWriterHandle,
    shutdown_handle: Option<JoinHandle<()>>,
    shutdown_trigger: Option<oneshot::Sender<()>>,
    flush_trigger: Arc<RwLock<Option<oneshot::Sender<()>>>>,
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
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let (flush_tx, flush_rx) = oneshot::channel();
        let flush_trigger = Arc::new(RwLock::new(Some(flush_tx)));

        let logger = Arc::new(RwLock::new(google_logger));
        let logger_clone = logger.clone();
        let flush_trigger_clone = flush_trigger.clone();

        let handle_task = tokio::spawn(async move {
            let mut buffer = Vec::with_capacity(config.max_batch);
            let mut flush_rx = flush_rx;

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

                    _ = &mut flush_rx => {
                        // Flush buffer on manual flush request
                        if !buffer.is_empty() {
                            Self::flush_batch(&logger_clone, std::mem::take(&mut buffer)).await;
                        }
                        // Signal completion to flush_and_wait
                        let _ = flush_rx.try_recv(); // not strictly necessary if using oneshot
                        // Recreate the flush channel for future flush requests
                        let (new_tx, new_rx) = oneshot::channel();
                        flush_rx = new_rx;
                        *flush_trigger_clone.write().await = Some(new_tx);
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
            flush_trigger,
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns a handle that implements [`std::io::Write`] for sending log entries.
    ///
    /// This handle can be cloned and shared across threads for synchronous logging.
    pub fn writer(&self) -> GoogleWriterHandle {
        self.handle.clone()
    }

    /// Immediately flushes all buffered logs.
    ///
    /// This sends a signal to the background task to flush the current buffer regardless
    /// of `max_batch` or `max_delay`. This is useful for ensuring logs are written
    /// before shutdown or at critical points in the application.
    pub async fn flush(&self) {
        if let Some(tx) = self.flush_trigger.write().await.take() {
            let _ = tx.send(()); // ignore error if background task is shutting down
        }
    }

    /// Flushes all buffered logs **and waits** until the background task completes the flush.
    pub async fn flush_and_wait(&self) {
        let (done_tx, done_rx) = oneshot::channel();

        {
            let mut flush_trigger = self.flush_trigger.write().await;
            *flush_trigger = Some(done_tx);
        }

        let tx_opt = {
            let mut flush_trigger = self.flush_trigger.write().await;
            flush_trigger.take()
        };

        if let Some(tx) = tx_opt {
            let _ = tx.send(());
        }

        let _ = done_rx.await;
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
    async fn flush_batch(logger: &Arc<RwLock<GoogleLogger<M>>>, batch: Vec<Value>) {
        let mut guard = logger.write().await;
        if let Err(err) = guard.write_logs(batch).await {
            tracing::error!("Failed to write log batch: {err}");
        }
    }
}
