use anyhow::Context;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace::Config, Resource};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialises `tracing_subscriber` with:
///
/// * env filter (defaults to `info,order_book=debug`)
/// * a formatting layer — JSON when `RUST_LOG_FORMAT=json`, pretty otherwise
/// * an OpenTelemetry layer that sends spans over OTLP gRPC to the endpoint
///   configured by `OTEL_EXPORTER_OTLP_ENDPOINT` (default `http://localhost:4317`)
pub fn init_tracing(service_name: &'static str) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,order_book=debug,tonic=info"));

    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(otlp_endpoint);

    let provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(Config::default().with_resource(Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", service_name),
        ])))
        .install_batch(runtime::Tokio)
        .context("install OTLP pipeline")?;

    let tracer = provider.tracer(service_name);
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let json_logs = std::env::var("RUST_LOG_FORMAT").ok().as_deref() == Some("json");
    if json_logs {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .with(otel_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .with(otel_layer)
            .init();
    }

    Ok(())
}
