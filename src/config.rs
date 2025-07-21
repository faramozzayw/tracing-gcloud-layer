use std::time::Duration;

#[derive(Debug, Clone)]
pub struct GoogleWriterConfig {
    pub max_batch: usize,
    pub max_delay: Duration,
    pub buffer_size: usize,
}

impl Default for GoogleWriterConfig {
    fn default() -> Self {
        Self {
            max_batch: 10,
            max_delay: Duration::from_secs(2),
            buffer_size: 1_000,
        }
    }
}

