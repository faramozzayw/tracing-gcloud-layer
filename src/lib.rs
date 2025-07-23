use derive_builder::Builder;
use google_logger::{GoogleLogger, LogMapper, LoggerError};
use tracing_subscriber::Registry;

use self::default_mapper::DefaultLogMapper;
use self::google_writer::GoogleWriter;

mod config;
mod default_mapper;
mod gauth;
pub mod google_logger;
pub mod google_writer;
mod log_entry;
mod utils;

pub use config::GoogleWriterConfig;
pub use utils::{extract_trace_id, get_severity};

#[derive(Builder, Clone)]
#[builder(pattern = "owned", setter(into, strip_option))]
pub struct GCloudLayerConfig<M: LogMapper = DefaultLogMapper> {
    log_name: String,
    logger_credential: Vec<u8>,
    #[builder(default)]
    config: GoogleWriterConfig,
    #[builder(default)]
    log_mapper: M,
}

impl<M: LogMapper> GCloudLayerConfig<M> {
    pub fn build_layer(
        self,
    ) -> Result<tracing_stackdriver::Layer<Registry, impl Fn() -> GoogleWriter<M>>, LoggerError>
    {
        let GCloudLayerConfig {
            config,
            log_mapper,
            log_name,
            logger_credential,
        } = self;

        let log_name = std::sync::Arc::from(log_name);
        let logger = GoogleLogger::new(log_name, logger_credential, log_mapper)?;

        Ok(tracing_stackdriver::layer()
            .with_writer(move || GoogleWriter::new(logger.clone(), config.clone())))
    }
}
