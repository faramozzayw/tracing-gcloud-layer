use serde_json::Value;
use std::io::Write;
use tokio::sync::mpsc;

/// Synchronous writer handle that can be used to send log entries to a background task.
///
/// This implements [`std::io::Write`] so it can be used as a sink for structured logs,
/// e.g., from `tracing` or other JSON-based logging systems. Log entries are sent
/// through an unbounded channel to a background batch writer.
#[derive(Clone)]
pub struct GoogleWriterHandle {
    pub(crate) sender: mpsc::UnboundedSender<Value>,
}

impl Write for GoogleWriterHandle {
    /// Sends a JSON log entry to the background batching task.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the log entry cannot be deserialized from JSON.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let log_entry: Value = serde_json::from_slice(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Err(e) = self.sender.send(log_entry) {
            tracing::error!("Failed to send log: {e}");
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
