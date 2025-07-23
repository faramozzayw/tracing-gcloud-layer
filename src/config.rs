use std::time::Duration;

use derive_builder::Builder;

const MAX_BATCH: usize = 10;
const BUFFER_SIZE: usize = 1_000;
const MAX_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned", setter(into, strip_option))]
pub struct GoogleWriterConfig {
    #[builder(default = MAX_BATCH)]
    pub max_batch: usize,
    #[builder(default =  MAX_DELAY)]
    pub max_delay: Duration,
    #[builder(default = BUFFER_SIZE)]
    pub buffer_size: usize,
}

impl Default for GoogleWriterConfig {
    fn default() -> Self {
        Self {
            max_batch: MAX_BATCH,
            max_delay: MAX_DELAY,
            buffer_size: BUFFER_SIZE,
        }
    }
}
