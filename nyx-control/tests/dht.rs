//! Integration test for full DHT put / get path.

#![cfg(feature = "dht")]

use nyx_control::spawn_dht;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dht_put_get_roundtrip() {
    // Spawn first DHT node (acts as root)
    let node1 = spawn_dht().await;

    // Spawn second DHT node and bootstrap to node1's first listen address.
    let node2 = spawn_dht().await;
    node2.add_bootstrap(node1.listen_addr().clone()).await;

    // Wait a bit for peers to connect.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Put a record from node1.
    let key = "test_key";
    let value = b"hello".to_vec();
    node1.put(key, value.clone()).await;

    // Allow propagate.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Attempt to fetch via node2; should retrieve the value.
    let fetched = node2.get(key).await;
    assert_eq!(fetched, Some(value));
} 