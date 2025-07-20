use google_writer::GoogleWriterConfig;
use tracing_subscriber::{Registry, layer::SubscriberExt};

use self::default_mapper::DefaultLogMapper;
use self::google_writer::GoogleWriter;

mod default_mapper;
mod gauth;
pub mod google_logger;
pub mod google_writer;
pub mod log_entry;
mod utils;

pub use utils::{extract_trace_id, get_severity};

pub fn get_google_logger_layer(
    log_name: &'static str,
    logger_credential: Vec<u8>,
) -> tracing_stackdriver::Layer<Registry, impl Fn() -> GoogleWriter<DefaultLogMapper>> {
    tracing_stackdriver::layer().with_writer(move || {
        GoogleWriter::new(
            log_name,
            logger_credential.to_owned(),
            GoogleWriterConfig::default(),
            DefaultLogMapper,
        )
    })
}

pub fn init_tracing(log_name: &'static str) {
    let logger_credential = std::env::var("LOGGER_CREDENTIAL").unwrap().into_bytes();
    let google_logger_layer = get_google_logger_layer(log_name, logger_credential);

    let subscriber = Registry::default()
        .with(google_logger_layer)
        .with(tracing_subscriber::fmt::layer());

    tracing::dispatcher::set_global_default(subscriber.into())
        .expect("Could not set up global logger");
}
