use nyx_telemetry::start_exporter;
use tokio::time::timeout;
use hyper::{Client, Uri};

#[tokio::test]
async fn exporter_returns_404() {
    // Start exporter on random port 0 -> not supported; pick 9900..
    let port = 9920;
    let handle = start_exporter(port);
    // Allow server to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let uri: Uri = format!("http://127.0.0.1:{}/notfound", port).parse().unwrap();
    let resp = Client::new().get(uri).await.unwrap();
    assert_eq!(resp.status(), 404);
    handle.abort();
} 