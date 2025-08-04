# tracing-gcloud-layer

A robust and configurable tracing layer for Rust, delivering logs from the [`tracing`](https://github.com/tokio-rs/tracing) ecosystem directly to [Google Cloud Logging (Stackdriver)](https://cloud.google.com/logging).

## 🚀 Features

- Seamless integration with Rust’s tracing ecosystem.
- Structured logs delivered to Google Cloud Logging (Stackdriver) using service account authentication.
- Asynchronous, batched log delivery for improved efficiency.
- Log formatting and enrichment customizable via the LogMapper trait.
- Basic support for trace ID and severity metadata propagation.

## 📦 Installation

Add to your `Cargo.toml`:

```sh
cargo add tracing-gcloud-layer
```

## 🛠️ Quickstart

1. **Generate a Google Cloud service account** with the "Logs Writer" role and download the JSON key.

2. **Initialize the tracing layer:**

```rust
use tracing_gcloud_layer::DefaultGCloudLayerConfigBuilder;
use tracing_subscriber::Registry;

let layer = DefaultGCloudLayerConfigBuilder::default()
    .log_name("my-service")
    .logger_credential(include_bytes!("../gcp-service-account.json"))
    .build()
    .expect("Invalid config")
    .build_layer();

tracing_subscriber::registry()
    .with(layer)
    .init();
```

3. **Emit logs in your application:**

```rust
tracing::info!(user_id = 42, "User logged in");
```

Logs will appear in Google Cloud Logging under the configured log name

## ⚙️ Configuration

- `log_name`: Log stream name in GCP (e.g., "stdout", "my-app").
- `logger_credential`: Service account credentials (as bytes).
- `config`: Batching, timeouts, and writer options.
- `log_mapper`: Plug in your own `LogMapper` to customize log transformation.

### Example: Custom Log Mapper

```rust
use tracing_gcloud_layer::{GCloudLayerConfigBuilder, LogContext, LogMapper};

#[derive(Clone, Default)]
pub struct CustomLogMapper;

impl LogMapper for CustomLogMapper {
    fn map(&self, context: LogContext, log_entry: serde_json::Value) -> serde_json::Value {
        todo!("Custom mapping logic here")
    }
}

let layer = GCloudLayerConfigBuilder::<CustomLogMapper>::default()
    .log_name("custom-logs")
    .logger_credential(include_bytes!("../gcp.json"))
    .log_mapper(CustomLogMapper)
    .build()
    .expect("Invalid config")
    .build_layer();
```
