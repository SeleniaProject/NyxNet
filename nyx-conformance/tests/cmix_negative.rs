#![allow(clippy::unwrap_used)]
use nyx_mix::cmix::{CmixController, verify_batch};
use tokio::time::Duration;

#[tokio::test]
async fn cmix_verify_rejects_tampered_batch() {
    let mut controller = CmixController::new(3, 10);
    let tx = controller.sender();
    for i in 0u8..3 {
        tx.send(vec![i]).await.unwrap();
    }
    // Waiting for batch emission within 100 ms.
    let mut batch = tokio::time::timeout(Duration::from_millis(100), controller.recv())
        .await
        .expect("batch timeout")
        .expect("controller closed");

    // Flip one bit of the digest – verification must fail.
    batch.digest[0] ^= 0xff;
    assert!(!verify_batch(&batch, controller.params(), None));
}

#[tokio::test]
async fn cmix_verify_rejects_invalid_witness() {
    let mut controller = CmixController::new(3, 10);
    let tx = controller.sender();
    for i in 0u8..3 {
        tx.send(vec![i + 10]).await.unwrap();
    }
    let mut batch = tokio::time::timeout(Duration::from_millis(100), controller.recv())
        .await
        .expect("batch timeout")
        .expect("controller closed");

    // Corrupt witness bytes.
    if let Some(b) = batch.witness.get_mut(0) {
        *b ^= 0x55;
    }
    assert!(!verify_batch(&batch, controller.params(), None));
} 