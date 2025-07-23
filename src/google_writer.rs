use serde_json::Value;
use std::{io::Write, pin::Pin, sync::Arc};
use tokio::{
    sync::{RwLock, mpsc, oneshot},
    task::JoinHandle,
    time::Sleep,
};

use super::google_logger::{GoogleLogger, LogMapper};
use crate::GoogleWriterConfig;

/// An asynchronous log writer that batches entries before sending them to Google Cloud Logging.
///
/// `GoogleWriter` is designed to be used in `tracing` or any logging setup where structured
/// JSON logs are sent to GCP. It runs a background task that receives logs through a channel
/// and flushes them in batches to reduce API calls.
///
/// Batching behavior is controlled via [`GoogleWriterConfig`] — you can tune the flush interval,
/// max batch size, and buffer limits.
pub struct GoogleWriter<M: LogMapper> {
    sender: mpsc::Sender<Value>,
    shutdown_trigger: Option<oneshot::Sender<()>>,
    shutdown_handle: Option<JoinHandle<()>>,
    _marker: std::marker::PhantomData<M>,
}

impl<M: LogMapper> GoogleWriter<M> {
    /// Creates a new `GoogleWriter` and spawns the background flush task.
    ///
    /// The task receives log entries from a channel, buffers them, and writes them
    /// either when:
    /// - the batch reaches `max_batch` entries, or
    /// - `max_delay` has elapsed since the first unflushed entry.
    ///
    /// The logger will also flush immediately during shutdown.
    pub fn new(google_logger: GoogleLogger<M>, config: GoogleWriterConfig) -> Self {
        let (tx, rx) = mpsc::channel::<Value>(config.buffer_size);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let logger = Arc::new(RwLock::new(google_logger));
        let handle = tokio::spawn(Self::run_batch_logger(rx, shutdown_rx, config, logger));

        Self {
            sender: tx,
            shutdown_trigger: Some(shutdown_tx),
            shutdown_handle: Some(handle),
            _marker: std::marker::PhantomData,
        }
    }

    /// Background task that receives log entries, batches them, and writes them to GCP.
    ///
    /// This loop exits cleanly when a shutdown signal is received.
    async fn run_batch_logger(
        mut receiver: mpsc::Receiver<Value>,
        mut shutdown: oneshot::Receiver<()>,
        config: GoogleWriterConfig,
        logger: Arc<RwLock<GoogleLogger<M>>>,
    ) {
        let mut buffer = Vec::with_capacity(config.max_batch);
        let mut flush_deadline: Option<Pin<Box<Sleep>>> = None;

        loop {
            tokio::select! {
                // Shutdown received
                _ = &mut shutdown => {
                    break;
                }

                // New log entry received
                Some(entry) = receiver.recv() => {
                    buffer.push(entry);

                    // Start the flush timer if this is the first entry
                    if flush_deadline.is_none() {
                        flush_deadline = Some(Box::pin(tokio::time::sleep(config.max_delay)));
                    }

                    // Flush immediately if batch size limit is hit
                    if buffer.len() >= config.max_batch {
                        Self::flush_batch(&logger, std::mem::take(&mut buffer)).await;
                        flush_deadline = None;
                    }
                }
                // Flush due to timeout
                _ = async {
                    if let Some(deadline) = &mut flush_deadline {
                        deadline.as_mut().await;
                    }
                }, if flush_deadline.is_some() => {
                    if !buffer.is_empty() {
                        Self::flush_batch(&logger, std::mem::take(&mut buffer)).await;
                    }
                    flush_deadline = None;
                }
            }
        }

        // final flush on shutdown
        if !buffer.is_empty() {
            Self::flush_batch(&logger, buffer).await;
        }

        tracing::debug!("Background task shut down cleanly.");
    }

    /// Flushes a batch of log entries to the Google Cloud Logging API.
    async fn flush_batch(logger: &Arc<RwLock<GoogleLogger<M>>>, batch: Vec<Value>) {
        let mut guard = logger.write().await;
        if let Err(err) = guard.write_logs(batch).await {
            tracing::error!("Failed to write log batch: {err}");
        }
    }
}

impl<M: LogMapper> Write for GoogleWriter<M> {
    /// Accepts a serialized JSON log entry and queues it for sending.
    ///
    /// If the internal channel is full, the log is dropped.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let log_entry: Value = serde_json::from_slice(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Err(e) = self.sender.try_send(log_entry) {
            tracing::warn!("Dropped log (channel full): {e}");
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // No-op, flushing handled by background task
        Ok(())
    }
}

impl<M: LogMapper> Drop for GoogleWriter<M> {
    /// Triggers shutdown of the background task and waits for it to complete.
    ///
    /// Ensures that any buffered logs are flushed before the writer is dropped.
    fn drop(&mut self) {
        tracing::debug!("GoogleWriter is being dropped; shutting down.");

        if let Some(shutdown_tx) = self.shutdown_trigger.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(handle) = self.shutdown_handle.take() {
            if let Err(err) =
                tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(handle))
            {
                tracing::error!("Shutdown task panicked: {:?}", err);
            }
        }
    }
}
