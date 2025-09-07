mod config;
mod default_mapper;
mod gauth;
pub mod google_logger;
pub mod google_writer;
mod log_entry;
mod runtime;
mod utils;

use derive_builder::Builder;
use google_logger::{GoogleLogger, LogMapper, LoggerError};
use runtime::GoogleWriterRuntime;
use tracing_subscriber::Registry;

use self::google_writer::GoogleWriterHandle;

pub use config::GoogleWriterConfig;
pub use default_mapper::DefaultLogMapper;
pub use utils::{extract_trace_id, get_severity};

pub type DefaultGCloudLayerConfig = GCloudLayerConfig<DefaultLogMapper>;
pub type DefaultGCloudLayerConfigBuilder = GCloudLayerConfigBuilder<DefaultLogMapper>;

/// Configuration for setting up Google Cloud logging with `tracing`.
///
/// `GCloudLayerConfig` holds everything needed to build a `tracing_stackdriver` layer
/// that sends logs to Google Cloud Logging. It supports custom log mappers, batching,
/// and uses service account credentials for authentication.
///
/// Use `.build_layer()` to produce a ready-to-use `tracing` layer.
#[derive(Builder, Clone)]
#[builder(pattern = "owned", setter(into, strip_option))]
pub struct GCloudLayerConfig<M: LogMapper = DefaultLogMapper> {
    /// The log name shown in Cloud Logging (e.g., `"stdout"` or `"my-service"`).
    log_name: String,
    /// Raw bytes of a Google service account JSON key.
    logger_credential: Vec<u8>,
    #[builder(default)]
    config: GoogleWriterConfig,
    #[builder(default)]
    log_mapper: M,
}

impl<M: LogMapper> GCloudLayerConfig<M> {
    /// Builds a `tracing_stackdriver` layer using this config.
    ///
    /// Creates a `GoogleLogger` from the provided log name and credentials,
    /// then wraps it in a `GoogleWriter` for async batching. Returns a
    /// `tracing_stackdriver` layer that can be added to a subscriber.
    ///
    /// # Example
    /// ```no_run
    /// use tracing_gcloud_layer::DefaultGCloudLayerConfigBuilder;
    /// use tracing_subscriber::prelude::*;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let svc_account_bytes = std::fs::read("svc-account.json")?;
    ///
    ///     let (layer, runtime) = DefaultGCloudLayerConfigBuilder::default()
    ///         .log_name("my-service")
    ///         .logger_credential(svc_account_bytes)
    ///         .build()?
    ///         .build_layer()?;
    ///
    ///     tracing_subscriber::registry().with(layer).init();
    ///     runtime.shutdown().await;
    ///     Ok(())
    /// }
    /// ```
    pub fn build_layer(
        self,
    ) -> Result<
        (
            tracing_stackdriver::Layer<Registry, impl Fn() -> GoogleWriterHandle>,
            GoogleWriterRuntime<M>,
        ),
        LoggerError,
    > {
        let GCloudLayerConfig {
            config,
            log_mapper,
            log_name,
            logger_credential,
        } = self;

        let log_name = std::sync::Arc::from(log_name);
        let logger = GoogleLogger::new(log_name, logger_credential, log_mapper)?;

        let runtime = GoogleWriterRuntime::new(logger.clone(), config.clone());

        let layer = tracing_stackdriver::layer().with_writer({
            let writer = runtime.writer();
            move || writer.clone()
        });

        Ok((layer, runtime))
    }
}
