//! Prometheus metrics exporter for Nyx.
//!
//! Provides a lightweight HTTP endpoint (`/metrics`) exposing the default
//! registry. Designed to run in its own Tokio task so callers can simply
//! invoke [`start_exporter`] at startup.

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use hyper::{Body, Method, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use once_cell::sync::Lazy;
use prometheus::{Encoder, TextEncoder, IntCounter, register_int_counter};
use tokio::task::JoinHandle;
use tracing::{error, info};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::{layer::SubscriberExt, Registry};

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
pub fn init_bunyan(service_name: &str) -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
    let formatting_layer = BunyanFormattingLayer::new(service_name.to_string(), std::io::stdout);
    let subscriber = Registry::default()
        .with(JsonStorageLayer)
        .with(formatting_layer);
    tracing::subscriber::set_global_default(subscriber)
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