use nyx_telemetry::{init_bunyan, start_exporter};

#[tokio::test]
async fn exporter_serves_metrics() {
    let _ = init_bunyan("nyx-test");
    let port = 9898;
    let handle = start_exporter(port);
    // Allow server to start
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let url = format!("http://127.0.0.1:{}/metrics", port);
    let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
    assert!(body.contains("nyx_exporter_requests_total"));

    handle.abort();
} 