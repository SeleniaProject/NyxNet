//! Prometheus metrics exporter for Nyx.
//!
//! Provides a lightweight HTTP endpoint (`/metrics`) exposing the default
//! registry. Designed to run in its own Tokio task so callers can simply
//! invoke [`start_exporter`] at startup.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use hyper::{Body, Method, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use once_cell::sync::Lazy;
use prometheus::{Encoder, TextEncoder, IntCounter, register_int_counter};
use tokio::task::JoinHandle;
use tracing::{error, info};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{layer::SubscriberExt, Registry};
use opentelemetry_sdk::{trace as sdktrace, Resource, runtime::Tokio};
use tracing_opentelemetry::OpenTelemetryLayer;

#[cfg(feature = "flamegraph")]
use std::{fs::File, time::{Duration, SystemTime, UNIX_EPOCH}};
#[cfg(feature = "flamegraph")]
use tokio::task::JoinHandle as TokioJoin;
#[cfg(feature = "flamegraph")]
use pprof::ProfilerGuard;

/// Total HTTP requests handled by the exporter itself.
static REQUEST_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("nyx_exporter_requests_total", "Total HTTP requests to Nyx exporter").expect("metric can be registered")
});

/// Increment the internal request counter.
#[inline]
pub fn inc_request_total() {
    REQUEST_TOTAL.inc();
}

/// Initialize Bunyan-formatted `tracing` subscriber for structured JSON logs.
/// Should be called once at application startup.
pub fn init_bunyan(service_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatting_layer = BunyanFormattingLayer::new(service_name.to_string(), std::io::stdout);
    // OpenTelemetry tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .with_trace_config(sdktrace::config().with_resource(Resource::new(vec![opentelemetry::KeyValue::new("service.name", service_name.to_string())])))
        .install_batch(Tokio)
        .map_err(|e| Box::<dyn std::error::Error + Send + Sync>::from(e))?;

    let otel_layer = OpenTelemetryLayer::new(tracer);

    let subscriber = Registry::default()
        .with(JsonStorageLayer)
        .with(formatting_layer)
        .with(otel_layer);
    tracing::subscriber::set_global_default(subscriber).map_err(|e| e.into())
}

/// Gracefully shutdown OpenTelemetry provider.
pub async fn shutdown_tracer() {
    opentelemetry::global::shutdown_tracer_provider();
}

/// Start the Prometheus exporter on `0.0.0.0:port` in a background task.
/// Returns the [`JoinHandle`] for the spawned task allowing the caller to await or cancel it.
pub fn start_exporter(port: u16) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Bind on all IPv4 addresses; dual-stack users can run another instance on v6 if needed.
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let make_svc = make_service_fn(|_conn| async {
            Ok::<_, hyper::Error>(service_fn(handle_request))
        });

        let server = Server::bind(&addr).serve(make_svc);
        info!("Prometheus exporter listening on {}", addr);

        if let Err(e) = server.await {
            error!("Exporter server error: {}", e);
        }
    })
}

#[cfg(feature = "flamegraph")]
#[cfg_attr(docsrs, doc(cfg(feature = "flamegraph")))]
/// Spawn a background profiler that dumps SVG flamegraphs every `interval` seconds to `output_dir`.
/// Each file is named `flamegraph_<unix_ts>.svg`.
pub fn start_flamegraph_dumper(output_dir: &str, interval: Duration) -> TokioJoin<()> {
    let dir = output_dir.to_string();
    tokio::spawn(async move {
        loop {
            let guard = ProfilerGuard::new(100).expect("create guard");
            tokio::time::sleep(interval).await;
            if let Ok(report) = guard.report().build() {
                // Create filename with current unix timestamp.
                let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                let path = format!("{}/flamegraph_{}.svg", dir, ts);
                match File::create(&path) {
                    Ok(file) => {
                        if report.flamegraph(file).is_ok() {
                            info!("flamegraph written to {}", path);
                        }
                    }
                    Err(e) => {
                        error!("failed to create flamegraph file: {}", e);
                    }
                }
            }
        }
    })
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => {
            inc_request_total();
            let metric_families = prometheus::gather();
            let encoder = TextEncoder::new();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer).expect("encoding metrics");
            Ok(Response::builder()
                .status(200)
                .header(hyper::header::CONTENT_TYPE, encoder.format_type())
                .body(Body::from(buffer))
                .expect("build response"))
        }
        _ => Ok(Response::builder()
            .status(404)
            .body(Body::from("Not Found"))
            .expect("build 404")),
    }
} 